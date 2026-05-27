use axum::extract::State;
use axum::middleware::Next;
use axum::response::IntoResponse;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::encoding::percent_decode;

/// Per-request CSRF token, populated by `web_auth_middleware` once the
/// requester's identity is resolved. Compared in constant time against the
/// `csrf_token` form field for every mutating /web/ POST.
#[derive(Clone)]
pub struct CsrfToken(pub String);

/// Domain-separation constant for the admin-token CSRF derivation. Bumping
/// this string rotates every admin-session CSRF token.
const ADMIN_CSRF_DERIVATION_LABEL: &[u8] = b"stackpit-csrf-v1";

#[derive(Clone)]
pub struct CsrfConfig {
    pub max_body_size: usize,
}

/// Deterministic CSRF token for admin_token sessions. Avoids a server-side
/// store; the admin_token is already the session secret, so deriving from
/// it is information-preserving.
pub fn derive_admin_csrf_token(admin_token: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(admin_token.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(ADMIN_CSRF_DERIVATION_LABEL);
    hex::encode(mac.finalize().into_bytes())
}

fn extract_csrf_field(body: &[u8]) -> Option<String> {
    let body_str = std::str::from_utf8(body).ok()?;
    for pair in body_str.split('&') {
        if let Some(val) = pair.strip_prefix("csrf_token=") {
            return Some(percent_decode(val.trim()));
        }
    }
    None
}

pub async fn csrf_middleware(
    State(config): State<CsrfConfig>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> axum::response::Response {
    let method = req.method().clone();
    // Cover every state-changing verb so new PUT/DELETE/PATCH routes can't bypass.
    let is_mutating = matches!(
        method,
        axum::http::Method::POST
            | axum::http::Method::PUT
            | axum::http::Method::DELETE
            | axum::http::Method::PATCH
    );
    let path = req.uri().path();
    let is_web = path.starts_with("/web/");
    let is_login = path == "/web/login";
    // OAuth login/callback are GET-only and pre-session; back-channel logout
    // is a server-to-server POST that verifies its own signature + JTI.
    let is_oauth_endpoint = path == "/web/auth/login"
        || path == "/web/auth/callback"
        || path == "/web/auth/backchannel-logout";

    // Bearer-authed callers carry no cookie, so CSRF doesn't apply to them; we
    // only guard cookie (browser) sessions on the JSON API.
    let has_bearer = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim_start().to_ascii_lowercase().starts_with("bearer "))
        .unwrap_or(false);
    let has_cookie = req.headers().contains_key(axum::http::header::COOKIE);
    let is_cookie_api = path.starts_with("/api/") && has_cookie && !has_bearer;

    let in_scope = (is_web && !is_login && !is_oauth_endpoint) || is_cookie_api;

    if !is_mutating || !in_scope {
        return next.run(req).await;
    }

    // Synchronizer pattern: web_auth_middleware ran first, so an authed
    // request must carry the token in extensions; missing => 403.
    let Some(session_token) = req.extensions().get::<CsrfToken>().map(|t| t.0.clone()) else {
        return (axum::http::StatusCode::FORBIDDEN, "CSRF token mismatch").into_response();
    };
    if session_token.is_empty() {
        return (axum::http::StatusCode::FORBIDDEN, "CSRF token mismatch").into_response();
    }

    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, config.max_body_size).await {
        Ok(b) => b,
        Err(_) => {
            return (axum::http::StatusCode::BAD_REQUEST, "request too large").into_response();
        }
    };

    let valid: bool = match extract_csrf_field(&bytes) {
        Some(field) if !field.is_empty() => session_token.as_bytes().ct_eq(field.as_bytes()).into(),
        _ => false,
    };

    if !valid {
        return (axum::http::StatusCode::FORBIDDEN, "CSRF token mismatch").into_response();
    }

    let req = axum::http::Request::from_parts(parts, axum::body::Body::from(bytes));
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_csrf_is_deterministic_per_token() {
        let a = derive_admin_csrf_token("secret-1");
        let b = derive_admin_csrf_token("secret-1");
        assert_eq!(a, b);
        let c = derive_admin_csrf_token("secret-2");
        assert_ne!(a, c);
    }

    #[test]
    fn admin_csrf_is_hex_sha256_wide() {
        // 32 bytes hex-encoded = 64 chars.
        assert_eq!(derive_admin_csrf_token("anything").len(), 64);
    }
}
