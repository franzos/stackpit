use ipnet::IpNet;
use std::net::IpAddr;
use std::str::FromStr;

/// CIDR block for IP-based filtering. Handles both v4 and v6.
#[derive(Clone)]
pub struct CidrBlock {
    net: IpNet,
}

impl CidrBlock {
    /// Parse CIDR notation or a plain address (plain = /32 or /128 host route).
    pub fn parse(s: &str) -> Option<Self> {
        let net = if s.contains('/') {
            IpNet::from_str(s).ok()?
        } else {
            // Bare address -> host route (/32 or /128).
            IpNet::from(IpAddr::from_str(s).ok()?)
        };
        Some(Self { net })
    }

    pub fn contains_addr(&self, ip: IpAddr) -> bool {
        match (ip, &self.net) {
            // IPv4-mapped IPv6 (::ffff:a.b.c.d) against a v4 network -- unwrap first.
            (IpAddr::V6(v6), IpNet::V4(_)) => match v6.to_ipv4_mapped() {
                Some(v4) => self.net.contains(&IpAddr::V4(v4)),
                None => false,
            },
            _ => self.net.contains(&ip),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr_contains() {
        let block = CidrBlock::parse("192.168.1.0/24").unwrap();
        assert!(block.contains_addr("192.168.1.100".parse().unwrap()));
        assert!(block.contains_addr("192.168.1.0".parse().unwrap()));
        assert!(block.contains_addr("192.168.1.255".parse().unwrap()));
        assert!(!block.contains_addr("192.168.2.1".parse().unwrap()));
    }

    #[test]
    fn cidr_single_host() {
        let block = CidrBlock::parse("10.0.0.1").unwrap();
        assert!(block.contains_addr("10.0.0.1".parse().unwrap()));
        assert!(!block.contains_addr("10.0.0.2".parse().unwrap()));
    }

    #[test]
    fn cidr_wide_block() {
        let block = CidrBlock::parse("10.0.0.0/8").unwrap();
        assert!(block.contains_addr("10.255.255.255".parse().unwrap()));
        assert!(!block.contains_addr("11.0.0.0".parse().unwrap()));
    }

    #[test]
    fn cidr_invalid() {
        assert!(CidrBlock::parse("not-an-ip").is_none());
        assert!(CidrBlock::parse("10.0.0.1/33").is_none());
    }

    #[test]
    fn cidr_ipv6() {
        let block = CidrBlock::parse("2001:db8::/32").unwrap();
        assert!(block.contains_addr("2001:db8::1".parse().unwrap()));
        assert!(block.contains_addr("2001:db8:ffff::1".parse().unwrap()));
        assert!(!block.contains_addr("2001:db9::1".parse().unwrap()));
    }

    #[test]
    fn cidr_ipv4_mapped_ipv6() {
        let block = CidrBlock::parse("192.168.1.0/24").unwrap();
        // Mapped v4 should match the v4 CIDR
        let mapped: IpAddr = "::ffff:192.168.1.100".parse().unwrap();
        assert!(block.contains_addr(mapped));
        // Different mapped v4 -- shouldn't match
        let other: IpAddr = "::ffff:10.0.0.1".parse().unwrap();
        assert!(!block.contains_addr(other));
        // Pure v6 against a v4 CIDR -- nope
        let pure_v6: IpAddr = "2001:db8::1".parse().unwrap();
        assert!(!block.contains_addr(pure_v6));
    }

    #[test]
    fn cidr_ipv6_single() {
        let block = CidrBlock::parse("::1").unwrap();
        assert!(block.contains_addr("::1".parse().unwrap()));
        assert!(!block.contains_addr("::2".parse().unwrap()));
    }
}
