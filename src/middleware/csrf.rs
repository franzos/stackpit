use axum::extract::State;
use axum::middleware::Next;
use axum::response::IntoResponse;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

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

/// CSRF token for admin_token sessions: `HMAC(admin_token, label || salt)`.
/// The salt is a per-login random value carried in a client cookie, so the
/// token is not a forever-valid function of admin_token alone -- knowing the
/// admin_token is not enough to precompute it.
pub fn derive_admin_csrf_token(admin_token: &str, salt: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(admin_token.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(ADMIN_CSRF_DERIVATION_LABEL);
    mac.update(salt.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// True when the Cookie header carries a stackpit session cookie (admin token
/// or OIDC grant, with or without the `__Host-` prefix). Bearer API clients
/// carry no such cookie and stay out of CSRF scope.
fn request_carries_session_cookie(cookie_header: &str) -> bool {
    use crate::html::login::{ADMIN_COOKIE, ADMIN_COOKIE_HOST};
    use crate::oidc::cookies::{GRANT_COOKIE, GRANT_COOKIE_HOST};

    cookie_header.split(';').any(|pair| {
        let name = pair.split('=').next().unwrap_or("").trim();
        matches!(
            name,
            GRANT_COOKIE | GRANT_COOKIE_HOST | ADMIN_COOKIE | ADMIN_COOKIE_HOST
        )
    })
}

/// Whether a mutating request to `path` (carrying a session cookie) needs the
/// CSRF synchronizer check. Carve-outs: login (pre-session), logout
/// (non-destructive + SameSite-protected), and the OAuth endpoints.
fn path_in_csrf_scope(path: &str, has_session_cookie: bool) -> bool {
    let is_web = path.starts_with("/web/");
    let is_login = path == "/web/login";
    let is_logout = path == "/web/logout";
    let is_oauth_endpoint = path == "/web/auth/login"
        || path == "/web/auth/callback"
        || path == "/web/auth/backchannel-logout";
    let is_cookie_api = path.starts_with("/api/") && has_session_cookie;
    (is_web && !is_login && !is_logout && !is_oauth_endpoint) || is_cookie_api
}

fn extract_csrf_field(body: &[u8]) -> Option<String> {
    form_urlencoded::parse(body)
        .find(|(k, _)| k == "csrf_token")
        .map(|(_, v)| v.into_owned())
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

    // Gate on a session cookie, not any cookie or an unvalidated Authorization
    // header: genuine bearer API clients carry no session cookie, so only
    // browser (cookie) sessions are guarded.
    let has_session_cookie = req
        .headers()
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(request_carries_session_cookie);

    let in_scope = path_in_csrf_scope(path, has_session_cookie);

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
    fn admin_csrf_is_deterministic_per_token_and_salt() {
        let a = derive_admin_csrf_token("secret-1", "salt-a");
        let b = derive_admin_csrf_token("secret-1", "salt-a");
        assert_eq!(a, b);
        let c = derive_admin_csrf_token("secret-2", "salt-a");
        assert_ne!(a, c);
    }

    #[test]
    fn admin_csrf_differs_across_salts() {
        let a = derive_admin_csrf_token("secret-1", "salt-a");
        let b = derive_admin_csrf_token("secret-1", "salt-b");
        assert_ne!(a, b, "distinct salts must yield distinct tokens");
    }

    #[test]
    fn logout_is_out_of_csrf_scope_but_other_web_posts_stay_in() {
        assert!(
            !path_in_csrf_scope("/web/logout", true),
            "logout must be exempt from the CSRF synchronizer check"
        );
        assert!(
            path_in_csrf_scope("/web/projects/", true),
            "normal mutating /web/ POSTs must stay in CSRF scope"
        );
        assert!(
            !path_in_csrf_scope("/web/login", true),
            "login is pre-session and must stay exempt"
        );
    }

    #[test]
    fn admin_csrf_is_hex_sha256_wide() {
        // 32 bytes hex-encoded = 64 chars.
        assert_eq!(derive_admin_csrf_token("anything", "salt").len(), 64);
    }
}
