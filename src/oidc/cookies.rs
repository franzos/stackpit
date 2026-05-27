//! Cookie shapes used by the OIDC browser flow.
//!
//! - `sp_grant`: opaque hex handle into [`super::grants`]. Primary auth cookie.
//! - `sp_login`: short-lived encrypted blob holding state + nonce + PKCE
//!   verifier between `/web/auth/login` and `/web/auth/callback`.
//!
//! Both HttpOnly. `SameSite=Strict` except the login cookie (Lax, since the
//! IdP callback crosses origins).

use axum::http::header::{HeaderValue, SET_COOKIE};
use axum::response::Response;

pub const GRANT_COOKIE: &str = "sp_grant";
pub const GRANT_COOKIE_HOST: &str = "__Host-sp_grant";
pub const LOGIN_COOKIE: &str = "sp_login";

/// `__Host-` prefix requires `Secure` + `Path=/` + no `Domain` (rules out
/// subdomain shadowing); only valid when cookies are Secure.
pub fn grant_cookie_name(secure: bool) -> &'static str {
    if secure {
        GRANT_COOKIE_HOST
    } else {
        GRANT_COOKIE
    }
}

/// Session cookie (no Max-Age): the server-side row drives lifetime.
pub fn build_grant_cookie(handle_hex: &str, secure: bool) -> HeaderValue {
    let name = grant_cookie_name(secure);
    let mut v = format!("{name}={handle_hex}; HttpOnly; SameSite=Strict; Path=/");
    if secure {
        v.push_str("; Secure");
    }
    HeaderValue::from_str(&v).expect("grant cookie value is ASCII")
}

pub fn clear_grant_cookie(secure: bool) -> HeaderValue {
    let name = grant_cookie_name(secure);
    let mut v = format!("{name}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0");
    if secure {
        v.push_str("; Secure");
    }
    HeaderValue::from_str(&v).expect("clear grant cookie value is ASCII")
}

/// 10-minute TTL mirrors Hydra's auth-code lifetime.
pub fn build_login_cookie(blob_b64: &str, secure: bool) -> HeaderValue {
    let mut v =
        format!("{LOGIN_COOKIE}={blob_b64}; HttpOnly; SameSite=Lax; Path=/web/auth/; Max-Age=600");
    if secure {
        v.push_str("; Secure");
    }
    HeaderValue::from_str(&v).expect("login cookie value is ASCII")
}

pub fn clear_login_cookie(secure: bool) -> HeaderValue {
    let mut v = format!("{LOGIN_COOKIE}=; HttpOnly; SameSite=Lax; Path=/web/auth/; Max-Age=0");
    if secure {
        v.push_str("; Secure");
    }
    HeaderValue::from_str(&v).expect("clear login cookie value is ASCII")
}

/// Append a `Set-Cookie` header without clobbering existing ones.
pub fn append_set_cookie(resp: &mut Response, value: HeaderValue) {
    resp.headers_mut().append(SET_COOKIE, value);
}
