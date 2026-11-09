//! Accès web public, borné et sans cookies. Le fournisseur de recherche est
//! explicite ; les pages ne bénéficient jamais de ses secrets ni de son réseau.

use async_trait::async_trait;
use hakille_core::Timestamp;
use hakille_kernel::research::{ResearchBackend, SearchHit, ToolFailure, WebPage};
use reqwest::{
    Url,
    header::{CONTENT_TYPE, HeaderMap, HeaderValue, LOCATION},
};
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

type WebResult<T> = std::result::Result<T, ToolFailure>;
const MAX_BODY: usize = 1_048_576;
const MAX_TEXT_CHARS: usize = 80_000;

enum SearchProvider {
    None,
    Brave { key: String },
    Searx { endpoint: Url },
}
pub struct WebResearch {
    provider: SearchProvider,
    transport: WebTransport,
}
#[derive(Default)]
struct WebTransport {
    proxy: Option<reqwest::Proxy>,
    certificates: Vec<reqwest::Certificate>,
}
impl WebTransport {
    fn from_env() -> Result<Self, String> {
        let proxy = std::env::var("HAKILLE_RESEARCH_PROXY")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|v| {
                let url = Url::parse(&v).map_err(|_| "proxy de recherche invalide")?;
                if !matches!(url.scheme(), "http" | "https") {
                    return Err("proxy HTTP(S) requis".to_string());
                }
                reqwest::Proxy::all(v).map_err(|_| "proxy de recherche invalide".to_string())
            })
            .transpose()?;
        let certificates = match std::env::var("HAKILLE_RESEARCH_CA_BUNDLE") {
            Ok(path) => {
                let bytes = std::fs::read(path).map_err(|_| "bundle CA de recherche illisible")?;
                let certificates = reqwest::Certificate::from_pem_bundle(&bytes)
                    .map_err(|_| "bundle CA de recherche invalide")?;
                if certificates.is_empty() {
                    return Err("bundle CA vide".into());
                }
                certificates
            }
            Err(_) => vec![],
        };
        Ok(Self {
            proxy,
            certificates,
        })
    }
}

impl WebResearch {
    pub fn from_env() -> Result<Self, String> {
        let brave = std::env::var("HAKILLE_BRAVE_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let searx = std::env::var("HAKILLE_SEARXNG_URL")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let provider = match (brave, searx) {
            (Some(_), Some(_)) => {
                return Err(
                    "choisir HAKILLE_BRAVE_API_KEY ou HAKILLE_SEARXNG_URL, pas les deux".into(),
                );
            }
            (Some(key), None) => {
                HeaderValue::from_str(&key).map_err(|_| "clé Brave invalide")?;
                SearchProvider::Brave { key }
            }
            (None, Some(value)) => {
                let endpoint = Url::parse(&value).map_err(|_| "URL SearXNG invalide")?;
                validate_configured_endpoint(&endpoint)?;
                SearchProvider::Searx { endpoint }
            }
            _ => SearchProvider::None,
        };
        Ok(Self {
            provider,
            transport: WebTransport::from_env()?,
        })
    }
}

fn validate_configured_endpoint(url: &Url) -> Result<(), String> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "SearXNG : URL HTTP(S) de /search requise, sans identifiants, paramètres ni fragment"
                .into(),
        );
    }
    // HTTP est réservé au service explicitement configuré sur la boucle locale.
    if url.scheme() == "http"
        && !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
    {
        return Err("SearXNG : HTTPS requis hors de la boucle locale".into());
    }
    Ok(())
}

pub(crate) fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let a = u32::from(v);
            let in_net = |base: [u8; 4], bits: u32| {
                (a >> (32 - bits)) == (u32::from(Ipv4Addr::from(base)) >> (32 - bits))
            };
            ![
                ([0, 0, 0, 0], 8),
                ([10, 0, 0, 0], 8),
                ([100, 64, 0, 0], 10),
                ([127, 0, 0, 0], 8),
                ([169, 254, 0, 0], 16),
                ([172, 16, 0, 0], 12),
                ([192, 0, 0, 0], 24),
                ([192, 0, 2, 0], 24),
                ([192, 88, 99, 0], 24),
                ([192, 168, 0, 0], 16),
                ([198, 18, 0, 0], 15),
                ([198, 51, 100, 0], 24),
                ([203, 0, 113, 0], 24),
                ([224, 0, 0, 0], 3),
            ]
            .iter()
            .any(|(base, bits)| in_net(*base, *bits))
        }
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let s = v.segments();
            // Unicast global uniquement ; refus des préfixes de transition,
            // de documentation et des affectations spéciales 2001::/23.
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
                && s[0] != 0x2002
                && !(s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

fn validate_public_url(url: &Url) -> WebResult<()> {
    if url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.as_str().len() > 2048
    {
        return Err(ToolFailure::permanent(
            "page refusée : HTTPS public sur le port 443, sans identifiants, requis",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| ToolFailure::permanent("hôte absent"))?
        .trim_matches(['[', ']']);
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        return Err(ToolFailure::permanent(
            "hôte local interdit aux lectures web",
        ));
    }
    if let Ok(ip) = host.parse::<IpAddr>()
        && !is_public_ip(ip)
    {
        return Err(ToolFailure::permanent(
            "adresse non publique interdite aux lectures web",
        ));
    }
    Ok(())
}

async fn public_addresses(url: &Url) -> WebResult<Vec<SocketAddr>> {
    validate_public_url(url)?;
    let host = url
        .host_str()
        .expect("hôte validé")
        .trim_matches(['[', ']']);
    let addresses: Vec<_> = tokio::net::lookup_host((host, 443))
        .await
        .map_err(|_| ToolFailure::transient("résolution DNS impossible"))?
        .collect();
    if addresses.is_empty() || addresses.iter().any(|a| !is_public_ip(a.ip())) {
        return Err(ToolFailure::permanent(
            "DNS refusé : adresse privée, réservée ou absente",
        ));
    }
    Ok(addresses)
}

fn transport_error(error: reqwest::Error) -> ToolFailure {
    // Aucun en-tête secret ni URL de requête dans l'erreur exposée.
    let retryable = error.is_timeout() || error.is_connect();
    ToolFailure {
        message: format!("transport HTTP : {}", error.without_url()),
        retryable,
    }
}

async fn get_once(
    url: &Url,
    headers: HeaderMap,
    configured_endpoint: bool,
    transport: &WebTransport,
) -> WebResult<reqwest::Response> {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(Duration::from_secs(8))
        .connect_timeout(Duration::from_secs(4))
        .user_agent("Hakille/0.1 (recherche explicite; lecture seule)");
    // Aucun proxy ambiant implicite. Le proxy explicitement configuré est
    // une frontière de confiance réseau : sa résolution finale est déléguée.
    if let Some(proxy) = &transport.proxy {
        builder = builder.proxy(proxy.clone());
    }
    for certificate in &transport.certificates {
        builder = builder.add_root_certificate(certificate.clone());
    }
    if !configured_endpoint {
        let addresses = public_addresses(url).await?;
        builder = builder.resolve_to_addrs(
            url.host_str()
                .expect("hôte validé")
                .trim_matches(['[', ']']),
            &addresses,
        );
    }
    builder
        .build()
        .map_err(transport_error)?
        .get(url.clone())
        .headers(headers)
        .send()
        .await
        .map_err(transport_error)
}

async fn bounded_body(mut response: reqwest::Response) -> WebResult<Vec<u8>> {
    let status = response.status();
    if !status.is_success() {
        return Err(ToolFailure {
            message: format!("HTTP {}", status.as_u16()),
            retryable: status.as_u16() == 429 || status.is_server_error(),
        });
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BODY as u64)
    {
        return Err(ToolFailure::permanent("réponse trop volumineuse (>1 Mio)"));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
        if body.len().saturating_add(chunk.len()) > MAX_BODY {
            return Err(ToolFailure::permanent("réponse trop volumineuse (>1 Mio)"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn extract_html(body: &str) -> (String, String) {
    let html = Html::parse_document(body);
    let title_sel = Selector::parse("title").expect("sélecteur statique");
    let title = html
        .select(&title_sel)
        .next()
        .map(|e| e.text().collect::<String>())
        .unwrap_or_default();
    let mut output = String::new();
    for node in html.root_element().descendants() {
        let Some(text) = node.value().as_text() else {
            continue;
        };
        let hidden = node.ancestors().any(|a| {
            a.value().as_element().is_some_and(|e| {
                matches!(
                    e.name(),
                    "script" | "style" | "noscript" | "template" | "head" | "svg"
                ) || e.attr("hidden").is_some()
                    || e.attr("aria-hidden") == Some("true")
            })
        });
        if !hidden {
            output.extend(
                text.chars()
                    .filter(|c| !c.is_control() || c.is_whitespace()),
            );
            output.push(' ');
        }
    }
    (
        title.split_whitespace().collect::<Vec<_>>().join(" "),
        output.split_whitespace().collect::<Vec<_>>().join(" "),
    )
}

#[async_trait]
impl ResearchBackend for WebResearch {
    fn name(&self) -> &str {
        match &self.provider {
            SearchProvider::None => "HTTPS (moteur de recherche non configuré)",
            SearchProvider::Brave { .. } => "Brave Search",
            SearchProvider::Searx { .. } => "SearXNG",
        }
    }
    fn transport_description(&self) -> &str {
        if self.transport.proxy.is_some() {
            "proxy explicitement configuré ; adresses contrôlées localement, résolution et connexion finales déléguées au proxy de confiance"
        } else {
            "direct ; DNS public épinglé, sans proxy ambiant"
        }
    }
    async fn search(&self, query: &str, limit: usize) -> WebResult<Vec<SearchHit>> {
        let (mut url, mut headers, configured) = match &self.provider {
            SearchProvider::None => {
                return Err(ToolFailure::permanent(
                    "recherche indisponible : configurer HAKILLE_BRAVE_API_KEY ou HAKILLE_SEARXNG_URL ; :read-web reste disponible",
                ));
            }
            SearchProvider::Brave { key } => {
                let mut headers = HeaderMap::new();
                let mut value = HeaderValue::from_str(key)
                    .map_err(|_| ToolFailure::permanent("clé Brave invalide"))?;
                value.set_sensitive(true);
                headers.insert("X-Subscription-Token", value);
                (
                    Url::parse("https://api.search.brave.com/res/v1/web/search")
                        .expect("URL statique"),
                    headers,
                    false,
                )
            }
            SearchProvider::Searx { endpoint } => (endpoint.clone(), HeaderMap::new(), true),
        };
        headers.insert("Accept", HeaderValue::from_static("application/json"));
        url.query_pairs_mut().append_pair("q", query);
        match self.provider {
            SearchProvider::Brave { .. } => {
                url.query_pairs_mut()
                    .append_pair("count", &limit.min(5).to_string());
            }
            _ => {
                url.query_pairs_mut()
                    .append_pair("format", "json")
                    .append_pair("safesearch", "1");
            }
        }
        // Aucune redirection du fournisseur : aucun risque d'envoyer sa clé ailleurs.
        let response = get_once(&url, headers, configured, &self.transport).await?;
        let body = bounded_body(response).await?;
        let json: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|_| ToolFailure::permanent("réponse de recherche JSON invalide"))?;
        let array = match self.provider {
            SearchProvider::Brave { .. } => json.pointer("/web/results"),
            _ => json.get("results"),
        }
        .and_then(|v| v.as_array())
        .ok_or_else(|| ToolFailure::permanent("schéma de résultats de recherche invalide"))?;
        let mut hits = Vec::new();
        for item in array.iter().take(limit.min(5)) {
            let Some(url) = item.get("url").and_then(|v| v.as_str()) else {
                continue;
            };
            let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let snippet = item
                .get("description")
                .or_else(|| item.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            hits.push(SearchHit {
                url: url.into(),
                title: extract_html(title).1,
                snippet: extract_html(snippet).1,
            });
        }
        Ok(hits)
    }
    async fn read(&self, raw: &str) -> WebResult<WebPage> {
        let mut url = Url::parse(raw).map_err(|_| ToolFailure::permanent("URL invalide"))?;
        url.set_fragment(None);
        let mut visited = std::collections::HashSet::new();
        for _ in 0..=3 {
            validate_public_url(&url)?;
            if !visited.insert(url.to_string()) {
                return Err(ToolFailure::permanent("boucle de redirections"));
            }
            let response = get_once(&url, HeaderMap::new(), false, &self.transport).await?;
            if response.status().is_redirection() {
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|h| h.to_str().ok())
                    .ok_or_else(|| ToolFailure::permanent("redirection sans destination valide"))?;
                url = url
                    .join(location)
                    .map_err(|_| ToolFailure::permanent("destination de redirection invalide"))?;
                url.set_fragment(None);
                continue;
            }
            let mime = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|h| h.to_str().ok())
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if response.status().is_success()
                && !matches!(
                    mime.as_str(),
                    "text/html" | "application/xhtml+xml" | "text/plain"
                )
            {
                return Err(ToolFailure::permanent(format!(
                    "format non pris en charge : {mime} (HTML ou texte requis)"
                )));
            }
            let bytes = bounded_body(response).await?;
            let content_sha256 = format!("{:x}", Sha256::digest(&bytes));
            let body = String::from_utf8(bytes)
                .map_err(|_| ToolFailure::permanent("page non UTF-8 non prise en charge"))?;
            let (title, text) = if mime == "text/plain" {
                (url.host_str().unwrap_or("").to_owned(), body)
            } else {
                extract_html(&body)
            };
            let text: String = text
                .chars()
                .filter(|c| !c.is_control() || c.is_whitespace())
                .collect();
            let title: String = title.chars().filter(|c| !c.is_control()).collect();
            let truncated = text.chars().count() > MAX_TEXT_CHARS;
            return Ok(WebPage {
                title,
                url: url.to_string(),
                text: text.chars().take(MAX_TEXT_CHARS).collect(),
                retrieved_at: Timestamp::now(),
                content_sha256,
                truncated,
            });
        }
        Err(ToolFailure::permanent("trop de redirections (maximum 3)"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn web_blocks_internal_and_reserved_destinations() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "172.16.2.3",
            "192.168.2.3",
            "100.64.0.1",
            "0.0.0.0",
            "198.18.0.1",
            "192.0.2.1",
            "224.0.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
            "2002:7f00:1::",
        ] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(is_public_ip(ip.parse().unwrap()));
        }
        for raw in [
            "file:///etc/passwd",
            "http://example.org",
            "https://localhost",
            "https://user:secret@example.org",
            "https://example.org:444",
            "https://2130706433",
            "https://[::1]",
        ] {
            assert!(
                validate_public_url(&Url::parse(raw).unwrap()).is_err(),
                "{raw}"
            );
        }
    }
    #[test]
    fn web_html_parser_excludes_active_and_hidden_content() {
        let (title, text) = extract_html(
            "<title>Source &amp; test</title><body><script>steal()</script><style>secret</style><div hidden>caché</div><p>Texte &amp; preuve</p><div aria-hidden='true'>invisible</div><template>instruction</template><p>日本語</p>",
        );
        assert_eq!(title, "Source & test");
        assert_eq!(text, "Texte & preuve 日本語");
    }
    #[test]
    fn web_only_explicit_loopback_provider_allows_http() {
        assert!(
            validate_configured_endpoint(&Url::parse("http://127.0.0.1:8080/search").unwrap())
                .is_ok()
        );
        for raw in [
            "http://example.org/search",
            "https://user:pass@example.org/search",
            "https://example.org/search?key=secret",
        ] {
            assert!(validate_configured_endpoint(&Url::parse(raw).unwrap()).is_err());
        }
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn serve_once(response: String) -> (Url, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let job = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = vec![0u8; 8192];
            let count = stream.read(&mut bytes).await.unwrap();
            // Le client peut refuser dès les en-têtes : broken pipe attendu.
            let _ = stream.write_all(response.as_bytes()).await;
            String::from_utf8_lossy(&bytes[..count]).into_owned()
        });
        (
            Url::parse(&format!("http://{address}/search")).unwrap(),
            job,
        )
    }

    #[tokio::test]
    async fn web_search_sends_only_query_to_explicit_provider_and_parses_response() {
        let body = r#"{"results":[{"title":"Page &amp; source","url":"https://example.org/","content":"<b>Extrait</b>"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (endpoint, server) = serve_once(response).await;
        let backend = WebResearch {
            provider: SearchProvider::Searx { endpoint },
            transport: WebTransport::default(),
        };
        let hits = backend.search("mémoire & raisonnement", 3).await.unwrap();
        let request = server.await.unwrap();
        assert!(request.contains("q=m%C3%A9moire+%26+raisonnement"));
        assert!(request.contains("format=json"));
        assert!(!request.to_lowercase().contains("authorization:"));
        assert!(!request.to_lowercase().contains("cookie:"));
        assert_eq!(hits[0].title, "Page & source");
        assert_eq!(hits[0].snippet, "Extrait");
    }
    #[tokio::test]
    async fn web_provider_redirect_is_refused_without_forwarding_secrets() {
        let (endpoint, server) = serve_once("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()).await;
        let backend = WebResearch {
            provider: SearchProvider::Searx { endpoint },
            transport: WebTransport::default(),
        };
        let error = backend.search("question", 3).await.unwrap_err();
        assert_eq!(error.message, "HTTP 302");
        assert!(!error.retryable);
        server.await.unwrap();
    }
    #[tokio::test]
    async fn web_response_limits_cover_declared_and_streamed_bytes() {
        for response in [
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_BODY + 1
            ),
            format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
                MAX_BODY + 1,
                "x".repeat(MAX_BODY + 1)
            ),
        ] {
            let (url, server) = serve_once(response).await;
            let response = get_once(&url, HeaderMap::new(), true, &WebTransport::default())
                .await
                .unwrap();
            let error = bounded_body(response).await.unwrap_err();
            assert!(error.message.contains("volumineuse"), "{}", error.message);
            assert!(!error.retryable);
            server.await.unwrap();
        }
    }
    #[tokio::test]
    async fn web_public_read_never_uses_the_private_provider_exception() {
        let backend = WebResearch {
            provider: SearchProvider::None,
            transport: WebTransport::default(),
        };
        let error = backend
            .read("https://127.0.0.1/metadata")
            .await
            .unwrap_err();
        assert!(!error.retryable);
        assert!(error.message.contains("non publique"));
    }
}
