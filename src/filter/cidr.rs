use std::net::IpAddr;

/// CIDR block for IP-based filtering. Handles both v4 and v6 -- stores
/// everything as u128 internally so the math stays uniform.
#[derive(Clone)]
pub struct CidrBlock {
    network: u128,
    mask: u128,
    is_v4: bool,
}

impl CidrBlock {
    /// Parse CIDR notation or a plain address (plain = /32 or /128).
    pub fn parse(s: &str) -> Option<Self> {
        let (addr_str, prefix_len) = if let Some((addr, prefix)) = s.split_once('/') {
            let prefix: u32 = prefix.parse().ok()?;
            (addr, prefix)
        } else {
            let addr: IpAddr = s.parse().ok()?;
            let max_prefix = if addr.is_ipv4() { 32 } else { 128 };
            (s, max_prefix)
        };

        let addr: IpAddr = addr_str.parse().ok()?;
        let is_v4 = addr.is_ipv4();
        let max_prefix = if is_v4 { 32 } else { 128 };
        if prefix_len > max_prefix {
            return None;
        }

        let bits = match addr {
            IpAddr::V4(v4) => u32::from(v4) as u128,
            IpAddr::V6(v6) => u128::from(v6),
        };

        let mask = if prefix_len == 0 {
            0u128
        } else if is_v4 {
            ((!0u32) << (32 - prefix_len)) as u128
        } else {
            (!0u128) << (128 - prefix_len)
        };

        Some(Self {
            network: bits & mask,
            mask,
            is_v4,
        })
    }

    pub fn contains_addr(&self, ip: IpAddr) -> bool {
        let bits = match (ip, self.is_v4) {
            (IpAddr::V4(v4), true) => u32::from(v4) as u128,
            (IpAddr::V6(v6), false) => u128::from(v6),
            // IPv4-mapped IPv6 (::ffff:a.b.c.d) -- unwrap to v4 and check.
            (IpAddr::V6(v6), true) => {
                if let Some(v4) = v6.to_ipv4_mapped() {
                    u32::from(v4) as u128
                } else {
                    return false;
                }
            }
            _ => return false,
        };
        (bits & self.mask) == self.network
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
