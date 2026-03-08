use axum::http::HeaderMap;

use crate::encoding::percent_decode;

#[derive(Debug, Clone)]
pub struct SentryAuth {
    pub sentry_key: String,
}

/// Pulls the sentry key out of request headers -- tries X-Sentry-Auth first, then Authorization.
pub fn extract_from_header(headers: &HeaderMap) -> Option<SentryAuth> {
    let header_val = headers
        .get("X-Sentry-Auth")
        .or_else(|| headers.get("Authorization"))
        .and_then(|v| v.to_str().ok())?;

    parse_auth_header(header_val)
}

pub fn extract_from_query(query: Option<&str>) -> Option<SentryAuth> {
    let query = query?;
    for pair in query.split('&') {
        if let Some(key) = pair.strip_prefix("sentry_key=") {
            return Some(SentryAuth {
                sentry_key: percent_decode(key),
            });
        }
    }
    None
}

/// Cracks open a DSN string to get the auth key and project ID out of it.
pub fn extract_from_dsn(dsn: &str) -> Option<(SentryAuth, u64)> {
    let without_scheme = dsn
        .strip_prefix("https://")
        .or_else(|| dsn.strip_prefix("http://"))?;
    let (key, rest) = without_scheme.split_once('@')?;
    let project_str = rest.rsplit('/').find(|s| !s.is_empty())?;
    let project_id: u64 = project_str.parse().ok()?;
    Some((
        SentryAuth {
            sentry_key: key.to_string(),
        },
        project_id,
    ))
}

fn parse_auth_header(value: &str) -> Option<SentryAuth> {
    let payload = value
        .strip_prefix("Sentry ")
        .or_else(|| value.strip_prefix("sentry "))?;

    let mut sentry_key = None;

    for part in payload.split(',') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("sentry_key=") {
            sentry_key = Some(val.to_string());
        }
    }

    Some(SentryAuth {
        sentry_key: sentry_key?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn parse_auth_header_with_key_and_version() {
        let auth = parse_auth_header("Sentry sentry_key=abc123, sentry_version=7").unwrap();
        assert_eq!(auth.sentry_key, "abc123");
    }

    #[test]
    fn parse_auth_header_lowercase_prefix() {
        let auth = parse_auth_header("sentry sentry_key=key1").unwrap();
        assert_eq!(auth.sentry_key, "key1");
    }

    #[test]
    fn parse_auth_header_missing_prefix_returns_none() {
        assert!(parse_auth_header("Bearer token123").is_none());
    }

    #[test]
    fn parse_auth_header_missing_key_returns_none() {
        assert!(parse_auth_header("Sentry sentry_version=7").is_none());
    }

    #[test]
    fn extract_from_header_x_sentry_auth() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Sentry-Auth", "Sentry sentry_key=abc".parse().unwrap());
        let auth = extract_from_header(&headers).unwrap();
        assert_eq!(auth.sentry_key, "abc");
    }

    #[test]
    fn extract_from_header_authorization_fallback() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Sentry sentry_key=xyz".parse().unwrap());
        let auth = extract_from_header(&headers).unwrap();
        assert_eq!(auth.sentry_key, "xyz");
    }

    #[test]
    fn extract_from_header_missing_returns_none() {
        let headers = HeaderMap::new();
        assert!(extract_from_header(&headers).is_none());
    }

    #[test]
    fn extract_from_query_valid() {
        let auth = extract_from_query(Some("sentry_key=mykey&other=1")).unwrap();
        assert_eq!(auth.sentry_key, "mykey");
    }

    #[test]
    fn extract_from_query_url_encoded_key() {
        let auth = extract_from_query(Some("sentry_key=abc%3D123%26key")).unwrap();
        assert_eq!(auth.sentry_key, "abc=123&key");
    }

    #[test]
    fn extract_from_query_no_key() {
        assert!(extract_from_query(Some("foo=bar&baz=1")).is_none());
    }

    #[test]
    fn extract_from_query_none_input() {
        assert!(extract_from_query(None).is_none());
    }

    #[test]
    fn extract_from_dsn_https() {
        let (auth, project_id) =
            extract_from_dsn("https://abc123@o123.ingest.sentry.io/456").unwrap();
        assert_eq!(auth.sentry_key, "abc123");
        assert_eq!(project_id, 456);
    }

    #[test]
    fn extract_from_dsn_http() {
        let (auth, project_id) = extract_from_dsn("http://key@localhost:3000/42").unwrap();
        assert_eq!(auth.sentry_key, "key");
        assert_eq!(project_id, 42);
    }

    #[test]
    fn extract_from_dsn_invalid_scheme() {
        assert!(extract_from_dsn("ftp://key@host/1").is_none());
    }

    #[test]
    fn extract_from_dsn_no_project_id() {
        assert!(extract_from_dsn("https://key@host/notanumber").is_none());
    }

    #[test]
    fn extract_from_dsn_no_at_sign() {
        assert!(extract_from_dsn("https://noatsign/1").is_none());
    }

    #[test]
    fn extract_from_dsn_trailing_slash() {
        let (auth, project_id) = extract_from_dsn("https://key@host/42/").unwrap();
        assert_eq!(auth.sentry_key, "key");
        assert_eq!(project_id, 42);
    }
}
