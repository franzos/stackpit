//! Admin-token resolution and HTML/JSON response helpers. Session-cookie +
//! OAuth bearer paths live on the stackpit side and reuse `resolve_admin` as
//! the first probe.

use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

use crate::admin_token::AdminSessionStore;
use crate::bearer::extract_bearer;
use crate::context::AuthContext;
use crate::cookie::read_cookie;

/// Returns `None` if no admin credential was presented. The bearer arm
/// compares the raw admin token; the cookie arm looks the per-login handle
/// up in `sessions`. `cookie_name` lets callers pick `__Host-`-prefixed names.
pub fn resolve_admin(
    headers: &HeaderMap,
    expected: &str,
    cookie_name: &str,
    sessions: &AdminSessionStore,
) -> Option<AuthContext> {
    if let Some(bearer) = extract_bearer(headers) {
        if bearer.as_bytes().ct_eq(expected.as_bytes()).into() {
            return Some(AuthContext::Admin);
        }
    }
    if let Some(cookie_val) = read_cookie(headers, cookie_name) {
        if sessions.is_valid(cookie_val) {
            return Some(AuthContext::Admin);
        }
    }
    None
}

pub fn wants_html(headers: &HeaderMap) -> bool {
    headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.contains("text/html"))
}

pub fn json_unauthorized() -> Response {
    (
        axum::http::StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({ "error": "unauthorized" })),
    )
        .into_response()
}
