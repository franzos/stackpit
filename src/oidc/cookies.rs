//! Cookie shapes used by the OIDC browser flow.
//!
//! - `sp_grant`: opaque hex handle into [`super::grants`]. Primary auth cookie.
//! - `sp_login`: short-lived encrypted blob holding state + nonce + PKCE
//!   verifier between `/web/auth/login` and `/web/auth/callback`.
//!
//! Both HttpOnly + `SameSite=Lax`: the grant cookie is set on the IdP callback
//! redirect, so `Strict` would be withheld on the post-login navigation back
//! from Hydra and bounce the user to /web/login. Lax still blocks cross-site
//! POST/subresource sends; state-changing requests are CSRF-token protected.

use axum::http::header::{HeaderValue, SET_COOKIE};
use axum::response::Response;

pub const GRANT_COOKIE: &str = "sp_grant";
pub const GRANT_COOKIE_HOST: &str = "__Host-sp_grant";
pub const LOGIN_COOKIE: &str = "sp_login";
pub const LOGIN_COOKIE_HOST: &str = "__Host-sp_login";

/// `__Host-` prefix requires `Secure` + `Path=/` + no `Domain` (rules out
/// subdomain shadowing); only valid when cookies are Secure.
pub fn grant_cookie_name(secure: bool) -> &'static str {
    if secure {
        GRANT_COOKIE_HOST
    } else {
        GRANT_COOKIE
    }
}

/// Same `__Host-` rule as the grant cookie; the prefix forces `Path=/`.
pub fn login_cookie_name(secure: bool) -> &'static str {
    if secure {
        LOGIN_COOKIE_HOST
    } else {
        LOGIN_COOKIE
    }
}

fn login_cookie_path(secure: bool) -> &'static str {
    if secure {
        "/"
    } else {
        "/web/auth/"
    }
}

/// Session cookie (no Max-Age): the server-side row drives lifetime.
pub fn build_grant_cookie(handle_hex: &str, secure: bool) -> HeaderValue {
    let name = grant_cookie_name(secure);
    let mut v = format!("{name}={handle_hex}; HttpOnly; SameSite=Lax; Path=/");
    if secure {
        v.push_str("; Secure");
    }
    HeaderValue::from_str(&v).expect("grant cookie value is ASCII")
}

pub fn clear_grant_cookie(secure: bool) -> HeaderValue {
    let name = grant_cookie_name(secure);
    let mut v = format!("{name}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0");
    if secure {
        v.push_str("; Secure");
    }
    HeaderValue::from_str(&v).expect("clear grant cookie value is ASCII")
}

/// Clear both grant cookie name variants regardless of TLS posture, so a stale
/// opposite-posture cookie can't outlive logout. `__Host-` clears must carry
/// `Secure` to be accepted; the bare clear must not.
pub fn clear_grant_cookie_all_variants() -> [HeaderValue; 2] {
    let bare = format!("{GRANT_COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0");
    let host = format!("{GRANT_COOKIE_HOST}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0; Secure");
    [
        HeaderValue::from_str(&bare).expect("clear grant cookie value is ASCII"),
        HeaderValue::from_str(&host).expect("clear grant cookie value is ASCII"),
    ]
}

/// `Max-Age` is an advisory copy of the deadline sealed into the blob - one constant drives both.
pub fn build_login_cookie(blob_b64: &str, secure: bool) -> HeaderValue {
    let max_age = crate::oidc::login_state::LOGIN_TTL_SECONDS;
    let (name, path) = (login_cookie_name(secure), login_cookie_path(secure));
    let mut v =
        format!("{name}={blob_b64}; HttpOnly; SameSite=Lax; Path={path}; Max-Age={max_age}");
    if secure {
        v.push_str("; Secure");
    }
    HeaderValue::from_str(&v).expect("login cookie value is ASCII")
}

pub fn clear_login_cookie(secure: bool) -> HeaderValue {
    let (name, path) = (login_cookie_name(secure), login_cookie_path(secure));
    let mut v = format!("{name}=; HttpOnly; SameSite=Lax; Path={path}; Max-Age=0");
    if secure {
        v.push_str("; Secure");
    }
    HeaderValue::from_str(&v).expect("clear login cookie value is ASCII")
}

/// Append a `Set-Cookie` header without clobbering existing ones.
pub fn append_set_cookie(resp: &mut Response, value: HeaderValue) {
    resp.headers_mut().append(SET_COOKIE, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C11 (round-3 review): the login-state cookie had no `__Host-` form, so
    /// a sibling subdomain could plant or shadow it.
    #[test]
    fn secure_login_cookie_is_host_prefixed_on_root_path() {
        let set = build_login_cookie("blob", true);
        let set = set.to_str().unwrap();
        assert!(set.starts_with("__Host-sp_login=blob;"));
        assert!(set.contains("Path=/;") && set.contains("Secure"));
        assert!(!set.contains("Domain"));
        let clear = clear_login_cookie(true);
        assert!(clear.to_str().unwrap().starts_with("__Host-sp_login=;"));

        let plain = build_login_cookie("blob", false);
        let plain = plain.to_str().unwrap();
        assert!(plain.starts_with("sp_login=blob;"));
        assert!(plain.contains("Path=/web/auth/"));
    }
}
