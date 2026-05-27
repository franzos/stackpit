//! `resolve_auth_context` derives `AuthContext` from request credentials
//! and stuffs it into extensions. Admin-token resolution lives here;
//! session-cookie + OAuth bearer paths live on the stackpit side and
//! reuse `resolve_admin` as the first probe.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Request};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use subtle::ConstantTimeEq;

use crate::admin_token::hash_token_for_cookie;
use crate::bearer::extract_bearer;
use crate::context::AuthContext;
use crate::cookie::read_cookie;

/// State for the admin-token resolver. `None` disables admin-token auth.
#[derive(Clone, Debug, Default)]
pub struct AdminToken(pub Option<String>);

impl AdminToken {
    pub fn new(token: Option<String>) -> Self {
        Self(token)
    }

    pub fn as_deref(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

/// Returns `None` if no admin credential was presented. `cookie_name` is
/// plumbed so callers can pick `__Host-`-prefixed names.
pub fn resolve_admin(
    headers: &HeaderMap,
    expected: &str,
    cookie_name: &str,
) -> Option<AuthContext> {
    if let Some(bearer) = extract_bearer(headers) {
        if bearer.as_bytes().ct_eq(expected.as_bytes()).into() {
            return Some(AuthContext::Admin);
        }
    }
    if let Some(cookie_val) = read_cookie(headers, cookie_name) {
        let expected_hash = hash_token_for_cookie(expected);
        if cookie_val.as_bytes().ct_eq(expected_hash.as_bytes()).into() {
            return Some(AuthContext::Admin);
        }
    }
    None
}

/// Attaches an `AuthContext` to extensions when found. Never rejects --
/// enforcement is up to the typed extractors or wrapper middleware.
pub async fn resolve_auth_context(
    State(token): State<AdminToken>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    if let Some(expected) = token.as_deref() {
        if let Some(ctx) = resolve_admin(req.headers(), expected, "stackpit_token") {
            req.extensions_mut().insert(ctx);
        }
    }
    next.run(req).await
}

pub fn wants_html(headers: &HeaderMap) -> bool {
    headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.contains("text/html"))
}

pub fn login_form_response() -> Response {
    Redirect::to("/web/login").into_response()
}

pub fn json_unauthorized() -> Response {
    (
        axum::http::StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({ "error": "unauthorized" })),
    )
        .into_response()
}
