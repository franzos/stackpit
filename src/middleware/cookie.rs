/// Extract a named cookie value from request headers.
pub(crate) fn extract_cookie_value(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    for val in headers.get_all("cookie") {
        if let Ok(s) = val.to_str() {
            for pair in s.split(';') {
                let pair = pair.trim();
                if let Some(rest) = pair.strip_prefix(name) {
                    if let Some(token) = rest.strip_prefix('=') {
                        let t = token.trim().trim_matches('"');
                        if !t.is_empty() {
                            return Some(t.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}
