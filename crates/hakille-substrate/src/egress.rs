//! Honest network egress shared by all OS substrates.
//!
//! Every [`Substrate::http_request`](crate::Substrate::http_request)
//! implementation enforces the caller's `Network` capability (including
//! expiry) *before* delegating here. This module then applies the
//! [`EgressPolicy`] allowlist and performs the real HTTPS fetch with
//! [`reqwest`] (rustls TLS, 10 s timeout, 5 MiB body cap).
//!
//! Default is loopback-only: with an empty allowlist only `localhost` /
//! loopback IPs are reachable. Once an allowlist is configured, *only*
//! listed hosts are reachable — loopback included solely when explicitly
//! listed.

use crate::{HttpRequest, HttpResponse};
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

/// Whole-request timeout (connect + read) applied to the reqwest client.
pub const EGRESS_TIMEOUT_SECS: u64 = 10;
/// Nombre maximal de redirections, chacune soumise à la politique.
pub const EGRESS_MAX_REDIRECTS: usize = 10;
/// Maximum request *and* response body size. Anything larger is refused
/// with an explicit error instead of being silently truncated.
pub const EGRESS_MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

/// Host allowlist egress policy.
///
/// * Empty `allowlist` (default) ⇒ **loopback-only mode**: only
///   `localhost` / `127.0.0.0/8` / `::1` may be fetched.
/// * Non-empty `allowlist` ⇒ only the listed hosts may be fetched;
///   loopback is allowed solely when explicitly listed.
/// * `deny_by_default` is always `true` in practice: anything not matched
///   is refused. (Set to `false` only to permit unlisted hosts — not
///   recommended, kept for tests.)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EgressPolicy {
    pub allowlist: Vec<String>,
    pub deny_by_default: bool,
}

impl Default for EgressPolicy {
    /// Secure default: loopback-only, deny by default.
    fn default() -> Self {
        Self::loopback_only()
    }
}

impl EgressPolicy {
    /// Loopback-only policy (no external egress).
    pub fn loopback_only() -> Self {
        Self {
            allowlist: Vec::new(),
            deny_by_default: true,
        }
    }

    /// Policy permitting exactly `hosts` (hostnames or IP literals;
    /// `host:port` and full-URL forms are accepted and normalized to the
    /// host part). Deny by default.
    pub fn with_allowlist(hosts: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allowlist: hosts.into_iter().map(Into::into).collect(),
            deny_by_default: true,
        }
    }

    /// `true` when no allowlist is configured (loopback-only mode).
    pub fn is_loopback_only(&self) -> bool {
        self.allowlist.is_empty()
    }

    /// Pure host check (no network I/O): is `host` reachable under this policy?
    pub fn is_host_allowed(&self, host: &str) -> bool {
        let normalized = normalize_host(host);
        if self.allowlist.is_empty() {
            return is_loopback_host(&normalized);
        }
        if self
            .allowlist
            .iter()
            .any(|entry| normalize_allowlist_entry(entry) == normalized)
        {
            return true;
        }
        !self.deny_by_default
    }

    /// Validate a full request URL against scheme + allowlist rules.
    /// Returns the parsed URL on success, or an explicit
    /// `SubstrateError("egress denied: …")` naming the reason.
    pub fn check_url(&self, url_str: &str) -> hakille_core::Result<reqwest::Url> {
        use hakille_core::HakilleError;
        let url = reqwest::Url::parse(url_str).map_err(|e| {
            HakilleError::SubstrateError(format!("egress denied: invalid URL: {e}"))
        })?;
        match url.scheme() {
            "http" | "https" => {}
            scheme => {
                return Err(HakilleError::SubstrateError(format!(
                    "egress denied: URL scheme '{scheme}' not allowed (http/https only)"
                )));
            }
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(HakilleError::SubstrateError(
                "egress denied: identifiants intégrés à l'URL interdits".into(),
            ));
        }
        let host = url
            .host_str()
            .ok_or_else(|| HakilleError::SubstrateError("egress denied: URL has no host".into()))?;
        if url.scheme() == "http" && !is_loopback_host(host) {
            return Err(HakilleError::SubstrateError(format!(
                "egress denied: HTTP en clair interdit hors loopback pour '{host}' (HTTPS requis)"
            )));
        }
        if !self.is_host_allowed(host) {
            if self.allowlist.is_empty() {
                return Err(HakilleError::SubstrateError(format!(
                    "egress denied: host '{host}' is not loopback \
                     (loopback-only default; configure EgressPolicy allowlist to permit egress)"
                )));
            }
            return Err(HakilleError::SubstrateError(format!(
                "egress denied: host '{host}' not in egress allowlist"
            )));
        }
        Ok(url)
    }
}

/// Normalise les noms DNS et les littéraux IP (IPv6 avec ou sans crochets).
pub fn normalize_host(host: &str) -> String {
    let host = host.trim().trim_end_matches('.');
    let literal = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    match literal.parse::<IpAddr>() {
        Ok(ip) => ip.to_string(),
        Err(_) => host.to_ascii_lowercase(),
    }
}

/// `true` for `localhost`, `127.0.0.0/8`, and `::1` (bracketed or not).
pub fn is_loopback_host(host: &str) -> bool {
    let normalized = normalize_host(host);
    if normalized == "localhost" {
        return true;
    }
    let ip_str = normalized
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'));
    let ip_str = ip_str.unwrap_or(&normalized);
    match ip_str.parse::<IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

/// Vérifie qu'une adresse IP résolue est publiquement routable (bloque loopback,
/// RFC 1918, CGNAT, link-local/IMDS 169.254.0.0/16, multicast, ULA, IPv4-mapped privées).
fn is_public_unicast_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, _, _] = v4.octets();
            let is_cgnat = a == 100 && (64..=127).contains(&b);
            let is_benchmark = a == 198 && (b == 18 || b == 19);
            let is_reserved = a >= 240;
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || is_cgnat
                || is_benchmark
                || is_reserved)
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_public_unicast_ip(IpAddr::V4(mapped));
            }
            let seg0 = v6.segments()[0];
            let is_ula = (seg0 & 0xfe00) == 0xfc00;
            let is_link_local = (seg0 & 0xffc0) == 0xfe80;
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || is_ula
                || is_link_local)
        }
    }
}

fn normalize_allowlist_entry(entry: &str) -> String {
    let entry = entry.trim().trim_end_matches('.').to_ascii_lowercase();
    // Full-URL form (`https://example.com:8443/path`) ⇒ extract host.
    if let Some(after_scheme) = entry.split_once("://").map(|(_, rest)| rest) {
        let host_port = after_scheme
            .split('/')
            .next()
            .unwrap_or(after_scheme)
            .rsplit('@')
            .next()
            .unwrap_or(after_scheme);
        return normalize_host(strip_port(host_port));
    }
    normalize_host(strip_port(&entry))
}

/// Strip a `:port` suffix. IPv6 literals (`::1`, `[::1]`) are never
/// treated as `host:port`.
fn strip_port(host_port: &str) -> &str {
    let host_port = host_port.trim();
    if let Some(inner) = host_port
        .strip_prefix('[')
        .and_then(|s| s.split_once(']'))
        .map(|(inner, _)| inner)
    {
        return inner;
    }
    // More than one ':' ⇒ bare IPv6 literal, keep as-is.
    if host_port.matches(':').count() != 1 {
        return host_port;
    }
    match host_port.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => host_port,
    }
}

/// Résout `host:port` et valide toutes les IP résolues contre SSRF / DNS rebinding.
async fn resolve_and_validate_addrs(
    url: &reqwest::Url,
    policy: &EgressPolicy,
) -> hakille_core::Result<(String, Vec<std::net::SocketAddr>)> {
    use hakille_core::HakilleError;
    let host = url
        .host_str()
        .ok_or_else(|| HakilleError::SubstrateError("egress denied: URL has no host".into()))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| HakilleError::SubstrateError("egress denied: port inconnu".into()))?;
    let host_clean = normalize_host(host);
    let is_explicit_ip_literal = host_clean.parse::<IpAddr>().is_ok();
    let host_is_loopback = is_loopback_host(&host_clean);

    let resolved: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host_clean.as_str(), port))
        .await
        .map_err(|e| {
            HakilleError::SubstrateError(format!("egress fetch failed: résolution DNS: {e}"))
        })?
        .collect();
    if resolved.is_empty() {
        return Err(HakilleError::SubstrateError(
            "egress denied: résolution DNS vide".into(),
        ));
    }
    for addr in &resolved {
        let ip = addr.ip();
        if policy.is_loopback_only() || host_is_loopback {
            if !ip.is_loopback() {
                return Err(HakilleError::SubstrateError(format!(
                    "egress denied: hôte loopback résolu vers IP non-loopback ({ip})"
                )));
            }
        } else if !is_explicit_ip_literal && !is_public_unicast_ip(ip) {
            return Err(HakilleError::SubstrateError(format!(
                "egress denied: SSRF bloqué (IP privée/interne résolue {ip})"
            )));
        }
    }
    Ok((host_clean, resolved))
}

/// En-têtes interdits en entrée pour prévenir le HTTP smuggling / contournement d'hôte.
fn is_forbidden_request_header(name: &reqwest::header::HeaderName) -> bool {
    matches!(
        name.as_str(),
        "host"
            | "proxy-authorization"
            | "proxy-connection"
            | "transfer-encoding"
            | "content-length"
            | "te"
            | "upgrade"
    )
}

/// Real egress fetch: allowlist-checked HTTPS request via reqwest
/// (rustls TLS, 10 s timeout, 5 MiB cap, `.no_proxy()`, DNS pinning anti-rebinding).
/// Capability enforcement (Network scope + expiry) MUST happen in the caller beforehand.
pub async fn fetch_via_egress(
    req: HttpRequest,
    policy: &EgressPolicy,
) -> hakille_core::Result<HttpResponse> {
    use hakille_core::HakilleError;
    if req.body.len() > EGRESS_MAX_BODY_BYTES {
        return Err(HakilleError::SubstrateError(format!(
            "egress denied: request body ({} bytes) exceeds 5MiB limit",
            req.body.len()
        )));
    }
    let mut current_url = policy.check_url(&req.url)?;
    let mut current_method = parse_method(&req.method)?;
    let mut current_body = req.body;
    let initial_origin = (
        current_url.scheme().to_string(),
        normalize_host(current_url.host_str().unwrap_or("")),
        current_url.port_or_known_default(),
    );

    let mut validated_headers = Vec::with_capacity(req.headers.len());
    for (name, value) in &req.headers {
        let header_name =
            reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|e| {
                HakilleError::SubstrateError(format!("egress denied: invalid header name: {e}"))
            })?;
        if is_forbidden_request_header(&header_name) {
            return Err(HakilleError::SubstrateError(format!(
                "egress denied: en-tête interdit '{}'",
                header_name.as_str()
            )));
        }
        let header_value = reqwest::header::HeaderValue::from_str(value).map_err(|e| {
            HakilleError::SubstrateError(format!("egress denied: invalid header value: {e}"))
        })?;
        validated_headers.push((header_name, header_value));
    }

    let mut redirects_followed = 0usize;
    let mut response = loop {
        let (dns_host, addrs) = resolve_and_validate_addrs(&current_url, policy).await?;
        let mut client_builder = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(EGRESS_TIMEOUT_SECS));
        if dns_host.parse::<IpAddr>().is_err() {
            client_builder = client_builder.resolve_to_addrs(&dns_host, &addrs);
        }
        let client = client_builder.build().map_err(|e| {
            HakilleError::SubstrateError(format!("egress denied: failed to build TLS client: {e}"))
        })?;

        let same_origin = (
            current_url.scheme(),
            normalize_host(current_url.host_str().unwrap_or("")),
            current_url.port_or_known_default(),
        ) == (
            initial_origin.0.as_str(),
            initial_origin.1.clone(),
            initial_origin.2,
        );

        let mut builder = client.request(current_method.clone(), current_url.clone());
        for (header_name, header_value) in &validated_headers {
            if !same_origin
                && matches!(
                    header_name.as_str(),
                    "authorization" | "cookie" | "x-api-key"
                )
            {
                continue;
            }
            builder = builder.header(header_name.clone(), header_value.clone());
        }
        if !current_body.is_empty() {
            builder = builder.body(current_body.clone());
        }

        let resp = builder.send().await.map_err(|e| {
            HakilleError::SubstrateError(format!("egress fetch failed: {}", e.without_url()))
        })?;

        let status = resp.status();
        if status.is_redirection()
            && let Some(loc_hdr) = resp.headers().get(reqwest::header::LOCATION)
        {
            if redirects_followed >= EGRESS_MAX_REDIRECTS {
                return Err(HakilleError::SubstrateError(
                    "egress denied: trop de redirections".into(),
                ));
            }
            let loc_str = loc_hdr.to_str().map_err(|_| {
                HakilleError::SubstrateError("egress denied: Location invalide".into())
            })?;
            let next_url = current_url.join(loc_str).map_err(|e| {
                HakilleError::SubstrateError(format!(
                    "egress denied: URL de redirection invalide: {e}"
                ))
            })?;
            if current_url.scheme() == "https" && next_url.scheme() != "https" {
                return Err(HakilleError::SubstrateError(
                    "egress denied: redirection HTTPS vers HTTP interdite".into(),
                ));
            }
            let next_url = policy.check_url(next_url.as_str())?;
            if matches!(status.as_u16(), 301..=303)
                && !matches!(current_method, reqwest::Method::GET | reqwest::Method::HEAD)
            {
                current_method = reqwest::Method::GET;
                current_body.clear();
            }
            current_url = next_url;
            redirects_followed += 1;
            continue;
        }

        break resp;
    };

    let status_code = response.status().as_u16();
    if let Some(declared) = response.content_length()
        && declared > EGRESS_MAX_BODY_BYTES as u64
    {
        return Err(HakilleError::SubstrateError(format!(
            "egress denied: response Content-Length ({declared} bytes) exceeds 5MiB limit"
        )));
    }
    let mut headers = HashMap::new();
    for (name, value) in response.headers().iter() {
        headers.insert(name.to_string(), value.to_str().unwrap_or("").to_string());
    }
    // Ne jamais accumuler une réponse entière avant de vérifier son plafond :
    // Content-Length est facultatif et ne protège pas les réponses chunked.
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| {
        HakilleError::SubstrateError(format!(
            "egress denied: failed reading response body: {}",
            e.without_url()
        ))
    })? {
        if chunk.len() > EGRESS_MAX_BODY_BYTES - body.len() {
            return Err(HakilleError::SubstrateError(
                "egress denied: response body exceeds 5MiB limit".into(),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(HttpResponse {
        status_code,
        headers,
        body,
    })
}

fn parse_method(method: &str) -> hakille_core::Result<reqwest::Method> {
    match method.to_ascii_uppercase().as_str() {
        "GET" => Ok(reqwest::Method::GET),
        "POST" => Ok(reqwest::Method::POST),
        "PUT" => Ok(reqwest::Method::PUT),
        "DELETE" => Ok(reqwest::Method::DELETE),
        "PATCH" => Ok(reqwest::Method::PATCH),
        "HEAD" => Ok(reqwest::Method::HEAD),
        "OPTIONS" => Ok(reqwest::Method::OPTIONS),
        other => Err(hakille_core::HakilleError::SubstrateError(format!(
            "egress denied: unsupported HTTP method '{other}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_is_loopback_only_deny_by_default() {
        let policy = EgressPolicy::default();
        assert!(policy.is_loopback_only());
        assert!(policy.deny_by_default);
        assert_eq!(EGRESS_TIMEOUT_SECS, 10);
        assert_eq!(EGRESS_MAX_BODY_BYTES, 5 * 1024 * 1024);
    }

    #[test]
    fn loopback_hosts_allowed_without_allowlist() {
        let policy = EgressPolicy::loopback_only();
        for host in [
            "localhost",
            "LOCALHOST",
            "127.0.0.1",
            "127.0.0.2",
            "::1",
            "[::1]",
        ] {
            assert!(
                policy.is_host_allowed(host),
                "loopback host '{host}' must be allowed"
            );
        }
        for host in [
            "example.com",
            "10.0.0.1",
            "192.168.1.1",
            "0.0.0.0",
            "",
            "localhost.evil.com",
        ] {
            assert!(
                !policy.is_host_allowed(host),
                "non-loopback host '{host}' must be denied"
            );
        }
    }

    #[test]
    fn allowlist_permits_only_listed_hosts() {
        let policy = EgressPolicy::with_allowlist(["example.com", "api.hakille.dev:8443"]);
        assert!(policy.is_host_allowed("example.com"));
        assert!(policy.is_host_allowed("EXAMPLE.com."));
        assert!(policy.is_host_allowed("api.hakille.dev"));
        // Loopback is denied once an allowlist exists and omits it.
        assert!(!policy.is_host_allowed("127.0.0.1"));
        assert!(!policy.is_host_allowed("localhost"));
        assert!(!policy.is_host_allowed("evil.com"));

        let with_loopback = EgressPolicy::with_allowlist(["example.com", "127.0.0.1", "localhost"]);
        assert!(with_loopback.is_host_allowed("127.0.0.1"));
        assert!(with_loopback.is_host_allowed("localhost"));
        assert!(!with_loopback.is_host_allowed("evil.com"));
    }

    #[test]
    fn allowlist_entry_url_form_normalizes_to_host() {
        let policy = EgressPolicy::with_allowlist(["https://example.com:8443/v1"]);
        assert!(policy.is_host_allowed("example.com"));
        assert!(!policy.is_host_allowed("other.com"));
    }

    #[test]
    fn check_url_rejects_bad_scheme_and_missing_host() {
        let policy = EgressPolicy::loopback_only();
        assert!(policy.check_url("file:///etc/passwd").is_err());
        assert!(policy.check_url("ftp://example.com/x").is_err());
        assert!(policy.check_url("not a url").is_err());
        let err = policy
            .check_url("https://example.com/")
            .expect_err("external host must be denied in loopback-only mode");
        assert!(err.to_string().contains("not loopback"));

        let policy = EgressPolicy::with_allowlist(["example.com"]);
        assert!(policy.check_url("https://example.com/api").is_ok());
        let err = policy
            .check_url("https://evil.com/")
            .expect_err("unlisted host must be denied");
        assert!(err.to_string().contains("not in egress allowlist"));
    }

    #[test]
    fn camera_config_reports_unavailable() {
        let config = crate::CameraConfig {
            width: 1920,
            height: 1080,
            fps: 30,
            facing: crate::CameraFacing::Front,
        };
        assert!(!config.is_available());
        assert!(!crate::CameraConfig::is_available(&config));
    }

    #[test]
    fn thermal_unknown_is_explicit() {
        let unknown = crate::ThermalState::Unknown;
        assert!(unknown.is_unknown());
        assert!(!unknown.is_known());
        let known = crate::ThermalState {
            temperature_celsius: 42.0,
            throttled: false,
        };
        assert!(known.is_known());
        assert!(!known.is_unknown());
    }
}
