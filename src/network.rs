use std::net::IpAddr;

/// Figures out the real client IP. The thing is -- we only trust XFF/X-Real-IP
/// when the peer is loopback, i.e. there's a local reverse proxy in front of us.
/// Otherwise a remote client could just spoof those headers.
pub fn extract_client_ip(
    headers: &axum::http::HeaderMap,
    peer_addr: Option<std::net::SocketAddr>,
) -> Option<String> {
    let peer_ip = peer_addr.map(|a| a.ip());
    let peer_is_loopback = peer_ip.is_some_and(|ip| ip.is_loopback());

    if peer_is_loopback {
        // First entry in XFF is the original client
        if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = xff.split(',').next() {
                let trimmed = first.trim();
                if !trimmed.is_empty() && trimmed.parse::<IpAddr>().is_ok() {
                    return Some(trimmed.to_string());
                }
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

    #[test]
    fn extract_client_ip_xff_from_loopback() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.1, 70.41.3.18".parse().unwrap(),
        );
        let loopback: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, Some(loopback)),
            Some("203.0.113.1".to_string())
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
        // XFF gets ignored when peer isn't loopback
        assert_eq!(
            extract_client_ip(&headers, Some(remote)),
            Some("198.51.100.1".to_string())
        );
    }

    #[test]
    fn extract_client_ip_real_ip_from_loopback() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "198.51.100.42".parse().unwrap());
        let loopback: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, Some(loopback)),
            Some("198.51.100.42".to_string())
        );
    }

    #[test]
    fn extract_client_ip_fallback_to_connect() {
        let headers = axum::http::HeaderMap::new();
        let addr: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, Some(addr)),
            Some("127.0.0.1".to_string())
        );
    }

    #[test]
    fn extract_client_ip_none() {
        let headers = axum::http::HeaderMap::new();
        assert_eq!(extract_client_ip(&headers, None), None);
    }

    #[test]
    fn extract_client_ip_invalid_xff_rejected() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "not-an-ip, 10.0.0.1".parse().unwrap());
        let loopback: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        // Bogus IP in XFF -- falls through to the peer address
        assert_eq!(
            extract_client_ip(&headers, Some(loopback)),
            Some("127.0.0.1".to_string())
        );
    }

    #[test]
    fn extract_client_ip_invalid_real_ip_rejected() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "garbage".parse().unwrap());
        let loopback: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, Some(loopback)),
            Some("127.0.0.1".to_string())
        );
    }
}
