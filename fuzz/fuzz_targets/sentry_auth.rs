#![no_main]

use axum::http::{HeaderMap, HeaderValue};
use libfuzzer_sys::fuzz_target;
use stackpit::ingest::auth::{extract_from_dsn, extract_from_header, extract_from_query};

fn key_is_clean(key: &str) {
    // Tabs are legal header bytes and just fail the key lookup; line breaks
    // and NUL must never reach logs or the DB.
    assert!(!key.contains(['\r', '\n', '\0']));
}

fuzz_target!(|input: (&str, &[u8])| {
    let (s, raw) = input;

    if let Some((auth, project_id)) = extract_from_dsn(s) {
        // The accepted key is exactly the userinfo part of the DSN.
        assert!(!auth.sentry_key.contains('@'));
        assert!(s.contains(&format!("{}@", auth.sentry_key)));
        assert!(s.contains(&project_id.to_string()));
    }

    let _ = extract_from_query(Some(s));
    let _ = extract_from_query(None);

    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_bytes(raw) {
        headers.insert("x-sentry-auth", v);
    }
    if let Some(auth) = extract_from_header(&headers) {
        key_is_clean(&auth.sentry_key);
    }
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(s) {
        headers.insert("authorization", v);
    }
    if let Some(auth) = extract_from_header(&headers) {
        key_is_clean(&auth.sentry_key);
    }
});
