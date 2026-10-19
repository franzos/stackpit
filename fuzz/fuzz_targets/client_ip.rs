#![no_main]

use axum::http::{HeaderMap, HeaderValue};
use libfuzzer_sys::fuzz_target;
use stackpit::util::network::{extract_client_ip, TrustedProxies};

use std::net::{IpAddr, SocketAddr};

fuzz_target!(|input: (&[u8], &[u8], [u8; 4], bool)| {
    let (xff, real_ip, peer, loopback) = input;
    let trusted = TrustedProxies::parse(&["10.0.0.0/8".to_string(), "fd00::/8".to_string()])
        .expect("static proxy list parses");

    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_bytes(xff) {
        headers.insert("x-forwarded-for", v);
    }
    if let Ok(v) = HeaderValue::from_bytes(real_ip) {
        headers.insert("x-real-ip", v);
    }
    let peer_ip: IpAddr = if loopback {
        IpAddr::from([127, 0, 0, 1])
    } else {
        IpAddr::from(peer)
    };
    let peer_addr = SocketAddr::new(peer_ip, 4242);

    if let Some(ip) = extract_client_ip(&headers, Some(peer_addr), &trusted) {
        // Whatever comes back is a parseable address, never raw header text.
        let parsed: IpAddr = ip.parse().expect("client ip must be an IP literal");
        // Only a trusted peer can make us report something other than itself.
        if !trusted.is_trusted(peer_ip) {
            assert_eq!(parsed, peer_ip);
        }
    }
    let _ = extract_client_ip(&headers, None, &trusted);
});
