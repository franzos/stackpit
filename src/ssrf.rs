use std::net::{IpAddr, Ipv4Addr, SocketAddr};

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
        IpAddr::V4(v4) => is_private_v4(v4),
        IpAddr::V6(v6) => {
            // IPv4-mapped IPv6 (::ffff:x.x.x.x) -- check the inner v4 address
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_private_v4(&v4);
            }

            let segments = v6.segments();

            // 6to4 (2002::/16) carries an embedded IPv4 in segments[1..=2].
            // Re-check the embedded v4 so attackers can't hide 10.0.0.1 as
            // 2002:0a00:0001::
            if segments[0] == 0x2002 {
                let embedded = Ipv4Addr::new(
                    (segments[1] >> 8) as u8,
                    (segments[1] & 0xff) as u8,
                    (segments[2] >> 8) as u8,
                    (segments[2] & 0xff) as u8,
                );
                return is_private_v4(&embedded);
            }

            // NAT64 well-known prefix (64:ff9b::/96). Embedded v4 sits in
            // the last 32 bits (segments[6..=7]).
            if segments[0] == 0x0064
                && segments[1] == 0xff9b
                && segments[2] == 0
                && segments[3] == 0
                && segments[4] == 0
                && segments[5] == 0
            {
                let embedded = Ipv4Addr::new(
                    (segments[6] >> 8) as u8,
                    (segments[6] & 0xff) as u8,
                    (segments[7] >> 8) as u8,
                    (segments[7] & 0xff) as u8,
                );
                return is_private_v4(&embedded);
            }

            // Teredo (2001::/32). The client's public IPv4 is stored XOR'd
            // with 0xffff in segments[6..=7]. We decode and re-check so
            // an attacker can't hide 169.254.169.254 as
            // 2001:0000:....:5601:5601.
            if segments[0] == 0x2001 && segments[1] == 0x0000 {
                let hi = segments[6] ^ 0xffff;
                let lo = segments[7] ^ 0xffff;
                let embedded = Ipv4Addr::new(
                    (hi >> 8) as u8,
                    (hi & 0xff) as u8,
                    (lo >> 8) as u8,
                    (lo & 0xff) as u8,
                );
                return is_private_v4(&embedded);
            }

            // IPv4-compatible IPv6 (::0:v4). Deprecated form where the upper
            // 80 bits are zero and segments[5] is 0x0000 (distinguishes from
            // v4-mapped, which sets segments[5] to 0xffff). Still reachable
            // via crafted DNS, so decode + re-check.
            if segments[0] == 0
                && segments[1] == 0
                && segments[2] == 0
                && segments[3] == 0
                && segments[4] == 0
                && segments[5] == 0
                && !(segments[6] == 0 && segments[7] == 0)
            {
                let embedded = Ipv4Addr::new(
                    (segments[6] >> 8) as u8,
                    (segments[6] & 0xff) as u8,
                    (segments[7] >> 8) as u8,
                    (segments[7] & 0xff) as u8,
                );
                return is_private_v4(&embedded);
            }

            v6.is_loopback() || v6.is_unspecified() || {
                // fc00::/7 unique local, fe80::/10 link-local
                (segments[0] & 0xfe00) == 0xfc00 || (segments[0] & 0xffc0) == 0xfe80
            }
        }
    }
}

/// All the IPv4 ranges we refuse to talk to.
fn is_private_v4(v4: &Ipv4Addr) -> bool {
    let octets = v4.octets();
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_unspecified()
        || is_cgnat(v4)
        // 0.0.0.0/8 -- "this network", whole range is non-routable
        || octets[0] == 0
        // 255.255.255.255 limited broadcast
        || v4.is_broadcast()
        // 198.18.0.0/15 -- benchmarking range (RFC 2544)
        || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        // 240.0.0.0/4 -- reserved / class E (covers 255.255.255.255 too, but
        // we list is_broadcast() above for clarity)
        || octets[0] >= 240
}

/// CGNAT / shared address space (100.64.0.0/10) — used by carriers and
/// some cloud providers. Not safe for outbound webhooks.
fn is_cgnat(v4: &Ipv4Addr) -> bool {
    let octets = v4.octets();
    octets[0] == 100 && (octets[1] & 0xC0) == 64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    fn v4(s: &str) -> IpAddr {
        IpAddr::V4(s.parse::<Ipv4Addr>().unwrap())
    }

    fn v6(s: &str) -> IpAddr {
        IpAddr::V6(s.parse::<Ipv6Addr>().unwrap())
    }

    #[test]
    fn blocks_v4_private_and_reserved() {
        // 0.0.0.0/8
        assert!(is_private_ip(&v4("0.0.0.0")));
        assert!(is_private_ip(&v4("0.1.2.3")));
        // Broadcast
        assert!(is_private_ip(&v4("255.255.255.255")));
        // Benchmark 198.18.0.0/15
        assert!(is_private_ip(&v4("198.18.0.1")));
        assert!(is_private_ip(&v4("198.19.255.255")));
        // Class E 240.0.0.0/4
        assert!(is_private_ip(&v4("240.0.0.1")));
        // Classic RFC1918 / loopback / link-local / CGNAT
        assert!(is_private_ip(&v4("10.0.0.1")));
        assert!(is_private_ip(&v4("127.0.0.1")));
        assert!(is_private_ip(&v4("169.254.169.254")));
        assert!(is_private_ip(&v4("100.64.0.1")));
    }

    #[test]
    fn blocks_v6_private_and_embedded_v4() {
        // 6to4 wrapping link-local 169.254.169.254
        assert!(is_private_ip(&v6("2002:a9fe:a9fe::")));
        // NAT64 wrapping link-local 169.254.169.254
        assert!(is_private_ip(&v6("64:ff9b::a9fe:a9fe")));
        // v4-mapped (::ffff:x.x.x.x) wrapping link-local 169.254.169.254
        assert!(is_private_ip(&v6("::ffff:169.254.169.254")));
        // IPv4-compatible (deprecated, ::x.x.x.x) wrapping link-local
        // 169.254.169.254 -- segments[5] is 0x0000, not 0xffff.
        assert!(is_private_ip(&v6("::a9fe:a9fe")));
        // Link-local, unique-local, loopback
        assert!(is_private_ip(&v6("fe80::1")));
        assert!(is_private_ip(&v6("fc00::1")));
        assert!(is_private_ip(&v6("::1")));
    }

    #[test]
    fn teredo_decodes_embedded_v4() {
        // Teredo wrapping link-local 169.254.169.254:
        // a9fe XOR ffff = 5601 for both halves. The server/port fields
        // (segments 2..=5) are arbitrary; we only care about 6..=7.
        assert!(is_private_ip(&v6(
            "2001:0000:4136:e378:8000:63bf:5601:5601"
        )));
        // RFC 4380 documentation example: embeds 192.0.2.45 (TEST-NET-1,
        // publicly routable in the docs sense -- NOT in is_private_v4).
        // 3fff XOR ffff = c000 (192.0), fdd2 XOR ffff = 022d (2.45).
        assert!(!is_private_ip(&v6(
            "2001:0000:4136:e378:8000:63bf:3fff:fdd2"
        )));
    }

    #[test]
    fn allows_public_addresses() {
        // Public v4
        assert!(!is_private_ip(&v4("1.1.1.1")));
        assert!(!is_private_ip(&v4("8.8.8.8")));
        // Public v6 (Cloudflare, Google DNS)
        assert!(!is_private_ip(&v6("2606:4700:4700::1111")));
        assert!(!is_private_ip(&v6("2001:4860:4860::8888")));
    }
}
