use std::net::{IpAddr, SocketAddr};

/// Validated DNS resolution for a webhook URL.
/// Contains everything needed to pin reqwest's connection to the validated IP.
pub struct ResolvedWebhook {
    pub hostname: String,
    pub addr: SocketAddr,
}

/// Resolves a webhook URL and checks that none of its addresses point to
/// private/internal IPs. Returns the hostname and first safe SocketAddr so the
/// caller can pin reqwest via `resolve()` -- closing the TOCTOU gap where DNS
/// could return a different IP at connection time.
pub async fn check_ssrf(url: &str) -> Result<ResolvedWebhook, String> {
    let (hostname, host_port) = extract_host_and_host_port(url)
        .ok_or_else(|| "invalid URL: cannot extract host".to_string())?;

    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(&host_port)
        .await
        .map_err(|e| format!("cannot resolve webhook host: {e}"))?
        .collect();

    for addr in &addrs {
        if is_private_ip(&addr.ip()) {
            return Err("webhook URL must not point to private/internal addresses".to_string());
        }
    }

    let addr = addrs
        .into_iter()
        .next()
        .ok_or_else(|| "DNS returned no addresses for webhook host".to_string())?;

    Ok(ResolvedWebhook { hostname, addr })
}

/// Pulls the hostname and "host:port" out of a URL.
/// Returns (hostname, host_port) -- hostname is without port, host_port is for `lookup_host`.
fn extract_host_and_host_port(url: &str) -> Option<(String, String)> {
    let after_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;

    let authority = after_scheme.split('/').next()?;
    if authority.is_empty() {
        return None;
    }

    if authority.contains(':') {
        let hostname = authority.split(':').next()?.to_string();
        Some((hostname, authority.to_string()))
    } else if url.starts_with("https://") {
        Some((authority.to_string(), format!("{authority}:443")))
    } else {
        Some((authority.to_string(), format!("{authority}:80")))
    }
}

fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || is_cgnat(v4)
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped IPv6 (::ffff:x.x.x.x) -- check the inner v4 address
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_private_ip(&IpAddr::V4(v4));
            }
            v6.is_loopback() || v6.is_unspecified() || {
                let segments = v6.segments();
                // fc00::/7 unique local, fe80::/10 link-local
                (segments[0] & 0xfe00) == 0xfc00 || (segments[0] & 0xffc0) == 0xfe80
            }
        }
    }
}

/// CGNAT / shared address space (100.64.0.0/10) — used by carriers and
/// some cloud providers. Not safe for outbound webhooks.
fn is_cgnat(v4: &std::net::Ipv4Addr) -> bool {
    let octets = v4.octets();
    octets[0] == 100 && (octets[1] & 0xC0) == 64
}
