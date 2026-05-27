use axum::http::header::{HeaderMap, COOKIE};

/// Read a named cookie value from the request headers. Returns the first
/// non-empty match across all `Cookie` headers; surrounding quotes are
/// stripped. Borrows from the header, so no allocation.
pub fn read_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    for val in headers.get_all(COOKIE) {
        let Ok(s) = val.to_str() else { continue };
        for pair in s.split(';') {
            let pair = pair.trim();
            let Some(rest) = pair.strip_prefix(name) else {
                continue;
            };
            let Some(value) = rest.strip_prefix('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}
