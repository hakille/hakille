//! Anti-SSRF (Server-Side Request Forgery) network validator and DNS rebinding protector.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use thiserror::Error;

/// SSRF policy violation errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SsrfError {
    #[error("Address '{0}' is blocked: loopback or link-local address")]
    LoopbackBlocked(IpAddr),

    #[error("Address '{0}' is blocked: private network (RFC 1918 / ULA)")]
    PrivateNetworkBlocked(IpAddr),

    #[error("Address '{0}' is blocked: cloud instance metadata service")]
    CloudMetadataBlocked(IpAddr),

    #[error("Address '{0}' is blocked: multicast or reserved address")]
    ReservedAddressBlocked(IpAddr),

    #[error("DNS resolution yielded no valid public IP addresses for host '{0}'")]
    NoPublicAddresses(String),

    #[error("Failed to resolve hostname '{0}': {1}")]
    ResolutionFailed(String, String),
}

/// Validator implementing sovereign network egress perimeter defense.
pub struct AntiSsrfValidator;

impl AntiSsrfValidator {
    /// Validates an IP address against sovereign anti-SSRF policies.
    pub fn validate_ip(ip: IpAddr) -> Result<(), SsrfError> {
        match ip {
            IpAddr::V4(ipv4) => Self::validate_ipv4(ipv4),
            IpAddr::V6(ipv6) => {
                // Check if IPv4-mapped IPv6 address (::ffff:x.x.x.x)
                if let Some(mapped_v4) = ipv6.to_ipv4_mapped() {
                    return Self::validate_ipv4(mapped_v4);
                }
                // Check if deprecated IPv4-compatible IPv6 address (::x.x.x.x)
                // Note: ::1 (localhost) and :: (unspecified) are handled in validate_ipv6
                if ipv6 != Ipv6Addr::LOCALHOST
                    && ipv6 != Ipv6Addr::UNSPECIFIED
                    && let Some(compat_v4) = ipv6.to_ipv4()
                {
                    return Self::validate_ipv4(compat_v4);
                }
                Self::validate_ipv6(ipv6)
            }
        }
    }

    /// Resolves a host name, validates all resolved IP addresses, and returns the verified IP to bind.
    ///
    /// Fail-closed guarantee: If ANY resolved IP belongs to a blocked subnet, the entire request
    /// is rejected immediately, preventing multi-homed DNS rebinding exploits.
    pub async fn resolve_and_validate(host: &str) -> Result<IpAddr, SsrfError> {
        let trimmed = host.trim();
        let without_scheme = if let Some(rest) = trimmed.strip_prefix("http://") {
            rest
        } else if let Some(rest) = trimmed.strip_prefix("https://") {
            rest
        } else if let Some(rest) = trimmed.strip_prefix("HTTP://") {
            rest
        } else if let Some(rest) = trimmed.strip_prefix("HTTPS://") {
            rest
        } else {
            trimmed
        };

        // Isolate authority up to the first path/query/fragment/backslash separator
        let authority = without_scheme
            .split(['/', '?', '#', '\\'])
            .next()
            .unwrap_or(without_scheme);

        // Defend against RFC 3986 userinfo confusion (`http://public.com:80@127.0.0.1/`):
        // always inspect the target host after the last `@`, AND reject `@` even if the target
        // host would otherwise look public.
        let has_userinfo = authority.contains('@');
        let host_port_part = authority.rsplit('@').next().unwrap_or(authority);

        let clean_host = if host_port_part.starts_with('[') {
            if let Some(close_bracket) = host_port_part.find(']') {
                &host_port_part[1..close_bracket]
            } else {
                host_port_part
            }
        } else if host_port_part.parse::<IpAddr>().is_ok() {
            host_port_part
        } else {
            host_port_part.split(':').next().unwrap_or(host_port_part)
        };

        // Check if clean_host is a literal IP address first (so blocked IPs in userinfo URLs return the exact Blocked error)
        if let Ok(ip) = clean_host.parse::<IpAddr>() {
            Self::validate_ip(ip)?;
            if has_userinfo {
                return Err(SsrfError::ResolutionFailed(
                    host.to_string(),
                    "URL userinfo '@' is forbidden by anti-SSRF validator".to_string(),
                ));
            }
            return Ok(ip);
        }

        let lower_host = clean_host.trim_end_matches('.').to_ascii_lowercase();
        if lower_host == "localhost" || lower_host.ends_with(".localhost") {
            return Err(SsrfError::LoopbackBlocked(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        }

        if has_userinfo
            || clean_host.is_empty()
            || clean_host
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(SsrfError::ResolutionFailed(
                host.to_string(),
                "Invalid or forbidden characters/userinfo in authority".to_string(),
            ));
        }

        // Resolve DNS
        let host_port = format!("{clean_host}:80");
        let addresses: Vec<std::net::SocketAddr> = tokio::net::lookup_host(&host_port)
            .await
            .map_err(|e| SsrfError::ResolutionFailed(clean_host.to_string(), e.to_string()))?
            .collect();

        if addresses.is_empty() {
            return Err(SsrfError::NoPublicAddresses(clean_host.to_string()));
        }

        // Verify ALL resolved addresses
        for addr in &addresses {
            Self::validate_ip(addr.ip())?;
        }

        // Return first verified address
        Ok(addresses[0].ip())
    }

    fn validate_ipv4(ip: Ipv4Addr) -> Result<(), SsrfError> {
        let octets = ip.octets();

        // 127.0.0.0/8 (Loopback)
        if octets[0] == 127 {
            return Err(SsrfError::LoopbackBlocked(IpAddr::V4(ip)));
        }

        // 169.254.0.0/16 (Link-local & AWS/GCP/Azure Metadata 169.254.169.254)
        if octets[0] == 169 && octets[1] == 254 {
            if octets[2] == 169 && octets[3] == 254 {
                return Err(SsrfError::CloudMetadataBlocked(IpAddr::V4(ip)));
            }
            return Err(SsrfError::LoopbackBlocked(IpAddr::V4(ip)));
        }

        // 10.0.0.0/8 (RFC 1918 Private)
        if octets[0] == 10 {
            return Err(SsrfError::PrivateNetworkBlocked(IpAddr::V4(ip)));
        }

        // 172.16.0.0/12 (RFC 1918 Private: 172.16.0.0 - 172.31.255.255)
        if octets[0] == 172 && (16..=31).contains(&octets[1]) {
            return Err(SsrfError::PrivateNetworkBlocked(IpAddr::V4(ip)));
        }

        // 192.168.0.0/16 (RFC 1918 Private)
        if octets[0] == 192 && octets[1] == 168 {
            return Err(SsrfError::PrivateNetworkBlocked(IpAddr::V4(ip)));
        }

        // 100.64.0.0/10 (Carrier-Grade NAT RFC 6598)
        if octets[0] == 100 && (64..=127).contains(&octets[1]) {
            return Err(SsrfError::PrivateNetworkBlocked(IpAddr::V4(ip)));
        }

        // 0.0.0.0/8 (Current network / "this" network)
        if octets[0] == 0 {
            return Err(SsrfError::ReservedAddressBlocked(IpAddr::V4(ip)));
        }

        // 224.0.0.0/4 (Multicast: 224.0.0.0 - 239.255.255.255)
        if (224..=239).contains(&octets[0]) {
            return Err(SsrfError::ReservedAddressBlocked(IpAddr::V4(ip)));
        }

        // 240.0.0.0/4 (Reserved: 240.0.0.0 - 255.255.255.254) & 255.255.255.255 (Broadcast)
        if octets[0] >= 240 {
            return Err(SsrfError::ReservedAddressBlocked(IpAddr::V4(ip)));
        }

        Ok(())
    }

    fn validate_ipv6(ip: Ipv6Addr) -> Result<(), SsrfError> {
        // ::1 (Loopback)
        if ip == Ipv6Addr::LOCALHOST {
            return Err(SsrfError::LoopbackBlocked(IpAddr::V6(ip)));
        }

        // :: (Unspecified)
        if ip == Ipv6Addr::UNSPECIFIED {
            return Err(SsrfError::ReservedAddressBlocked(IpAddr::V6(ip)));
        }

        let segments = ip.segments();

        // fe80::/10 (Link-local unicast)
        if (segments[0] & 0xffc0) == 0xfe80 {
            return Err(SsrfError::LoopbackBlocked(IpAddr::V6(ip)));
        }

        // fc00::/7 (Unique Local Address - ULA: fc00.. and fd00..)
        if (segments[0] & 0xfe00) == 0xfc00 {
            return Err(SsrfError::PrivateNetworkBlocked(IpAddr::V6(ip)));
        }

        // ff00::/8 (Multicast)
        if (segments[0] & 0xff00) == 0xff00 {
            return Err(SsrfError::ReservedAddressBlocked(IpAddr::V6(ip)));
        }

        Ok(())
    }
}
