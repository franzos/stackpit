use crate::filter::cidr::CidrBlock;
use std::net::IpAddr;

/// Peers allowed to set forwarding headers. Loopback is always trusted;
/// additional proxies (Docker bridge, LB) come from `server.trusted_proxies`.
#[derive(Default)]
pub struct TrustedProxies {
    blocks: Vec<CidrBlock>,
}

impl TrustedProxies {
    /// Parse config entries: plain IPs (host route) or CIDR notation, v4 or v6.
    pub fn parse(entries: &[String]) -> Result<Self, String> {
        let mut blocks = Vec::with_capacity(entries.len());
        for entry in entries {
            let trimmed = entry.trim();
            let block = CidrBlock::parse(trimmed).ok_or_else(|| {
                format!(
                    "invalid server.trusted_proxies entry '{trimmed}': \
                     expected an IP address or CIDR block (e.g. \"10.0.0.5\" or \"172.16.0.0/12\")"
                )
            })?;
            blocks.push(block);
        }
        Ok(Self { blocks })
    }

    pub fn is_trusted(&self, ip: IpAddr) -> bool {
        ip.is_loopback() || self.blocks.iter().any(|b| b.contains_addr(ip))
    }
}

/// Extract the real client IP. Forwarding headers are honored only when the
/// TCP peer is trusted (loopback or in `trusted_proxies`). X-Forwarded-For is
/// walked right to left, skipping trusted proxies, because only the rightmost
/// entries were appended by proxies we control; the leftmost entries are
/// attacker-supplied. X-Real-IP is single-valued and used as a fallback.
pub fn extract_client_ip(
    headers: &axum::http::HeaderMap,
    peer_addr: Option<std::net::SocketAddr>,
    trusted: &TrustedProxies,
) -> Option<String> {
    let peer_ip = peer_addr.map(|a| a.ip());
    let peer_is_trusted = peer_ip.is_some_and(|ip| trusted.is_trusted(ip));

    if peer_is_trusted {
        if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            let mut leftmost_trusted: Option<IpAddr> = None;
            for entry in xff.split(',').rev() {
                let Ok(ip) = entry.trim().parse::<IpAddr>() else {
                    // Malformed entry breaks the verifiable chain; stop here.
                    break;
                };
                if trusted.is_trusted(ip) {
                    leftmost_trusted = Some(ip);
                } else {
                    return Some(ip.to_string());
                }
            }
            // Whole (verifiable) chain is trusted proxies: use the leftmost one.
            if let Some(ip) = leftmost_trusted {
                return Some(ip.to_string());
            }
        }

        if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
            let trimmed = real_ip.trim();
            if !trimmed.is_empty() && trimmed.parse::<IpAddr>().is_ok() {
                return Some(trimmed.to_string());
            }
        }
    }

    peer_ip.map(|ip| ip.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_proxies() -> TrustedProxies {
        TrustedProxies::default()
    }

    fn proxies(entries: &[&str]) -> TrustedProxies {
        let owned: Vec<String> = entries.iter().map(|s| (*s).to_string()).collect();
        TrustedProxies::parse(&owned).unwrap()
    }

    #[test]
    fn extract_client_ip_leftmost_spoof_rejected() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.1, 70.41.3.18".parse().unwrap(),
        );
        let loopback: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        // 70.41.3.18 was appended by our proxy; 203.0.113.1 is attacker-supplied.
        assert_eq!(
            extract_client_ip(&headers, Some(loopback), &no_proxies()),
            Some("70.41.3.18".to_string())
        );
    }

    #[test]
    fn extract_client_ip_xff_ignored_from_remote() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.1, 70.41.3.18".parse().unwrap(),
        );
        let remote: std::net::SocketAddr = "198.51.100.1:12345".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, Some(remote), &no_proxies()),
            Some("198.51.100.1".to_string())
        );
    }

    #[test]
    fn extract_client_ip_trusted_proxy_peer_honored() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "70.41.3.18".parse().unwrap());
        let peer: std::net::SocketAddr = "172.18.0.2:12345".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, Some(peer), &proxies(&["172.16.0.0/12"])),
            Some("70.41.3.18".to_string())
        );
    }

    #[test]
    fn extract_client_ip_untrusted_peer_ignores_headers() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "70.41.3.18".parse().unwrap());
        headers.insert("x-real-ip", "70.41.3.19".parse().unwrap());
        let peer: std::net::SocketAddr = "203.0.113.7:12345".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, Some(peer), &proxies(&["172.16.0.0/12"])),
            Some("203.0.113.7".to_string())
        );
    }

    #[test]
    fn extract_client_ip_skips_chained_trusted_proxies() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.1, 70.41.3.18, 172.18.0.5, 10.0.0.9"
                .parse()
                .unwrap(),
        );
        let loopback: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        // Rightmost trusted hops are skipped; first untrusted from the right wins.
        assert_eq!(
            extract_client_ip(
                &headers,
                Some(loopback),
                &proxies(&["172.16.0.0/12", "10.0.0.9"])
            ),
            Some("70.41.3.18".to_string())
        );
    }

    #[test]
    fn extract_client_ip_all_trusted_chain_uses_leftmost() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "172.18.0.5, 10.0.0.9".parse().unwrap());
        let loopback: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        assert_eq!(
            extract_client_ip(
                &headers,
                Some(loopback),
                &proxies(&["172.16.0.0/12", "10.0.0.0/8"])
            ),
            Some("172.18.0.5".to_string())
        );
    }

    #[test]
    fn extract_client_ip_real_ip_from_loopback() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "198.51.100.42".parse().unwrap());
        let loopback: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, Some(loopback), &no_proxies()),
            Some("198.51.100.42".to_string())
        );
    }

    #[test]
    fn extract_client_ip_real_ip_ignored_from_remote() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "198.51.100.42".parse().unwrap());
        let remote: std::net::SocketAddr = "198.51.100.1:12345".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, Some(remote), &no_proxies()),
            Some("198.51.100.1".to_string())
        );
    }

    #[test]
    fn extract_client_ip_fallback_to_connect() {
        let headers = axum::http::HeaderMap::new();
        let addr: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, Some(addr), &no_proxies()),
            Some("127.0.0.1".to_string())
        );
    }

    #[test]
    fn extract_client_ip_none() {
        let headers = axum::http::HeaderMap::new();
        assert_eq!(extract_client_ip(&headers, None, &no_proxies()), None);
    }

    #[test]
    fn extract_client_ip_valid_entry_right_of_malformed_still_used() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "not-an-ip, 70.41.3.18".parse().unwrap());
        let loopback: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        // The proxy-appended rightmost entry is valid; garbage to its left is ignored.
        assert_eq!(
            extract_client_ip(&headers, Some(loopback), &no_proxies()),
            Some("70.41.3.18".to_string())
        );
    }

    #[test]
    fn extract_client_ip_malformed_rightmost_falls_back_to_peer() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "70.41.3.18, garbage".parse().unwrap());
        let loopback: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, Some(loopback), &no_proxies()),
            Some("127.0.0.1".to_string())
        );
    }

    #[test]
    fn extract_client_ip_invalid_real_ip_rejected() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "garbage".parse().unwrap());
        let loopback: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, Some(loopback), &no_proxies()),
            Some("127.0.0.1".to_string())
        );
    }

    #[test]
    fn trusted_proxies_parse_rejects_malformed() {
        assert!(TrustedProxies::parse(&["not-an-ip".to_string()]).is_err());
        assert!(TrustedProxies::parse(&["10.0.0.1/33".to_string()]).is_err());
    }

    #[test]
    fn trusted_proxies_parse_accepts_plain_and_cidr() {
        let t = TrustedProxies::parse(&[
            "10.0.0.5".to_string(),
            "172.16.0.0/12".to_string(),
            "2001:db8::/32".to_string(),
        ])
        .unwrap();
        assert!(t.is_trusted("10.0.0.5".parse().unwrap()));
        assert!(!t.is_trusted("10.0.0.6".parse().unwrap()));
        assert!(t.is_trusted("172.31.255.1".parse().unwrap()));
        assert!(t.is_trusted("2001:db8::1".parse().unwrap()));
        assert!(t.is_trusted("127.0.0.1".parse().unwrap()));
        assert!(!t.is_trusted("203.0.113.1".parse().unwrap()));
    }
}
