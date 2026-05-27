use axum::extract::{Form, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use secrecy::ExposeSecret;
use serde::Deserialize;

use crate::server::AppState;

pub const ADMIN_COOKIE: &str = "stackpit_token";
pub const ADMIN_COOKIE_HOST: &str = "__Host-stackpit_token";

/// Pick the admin-token cookie name based on the deployment's TLS posture.
/// `__Host-` requires `Secure` + `Path=/` + no `Domain` -- we only use it
/// when cookies are Secure so the prefix's invariants hold.
pub fn admin_cookie_name(secure: bool) -> &'static str {
    if secure {
        ADMIN_COOKIE_HOST
    } else {
        ADMIN_COOKIE
    }
}

#[derive(askama::Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: Option<String>,
    oauth_enabled: bool,
    /// Info banner: e.g. `local` after a logout that couldn't reach the
    /// IdP's `end_session_endpoint`. Kept separate from `error` so the user
    /// gets neutral phrasing (this isn't a failure -- just a heads-up).
    info: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct LoginQuery {
    error: Option<String>,
    /// `logout=local` means we ran a local-only logout (the IdP discovery
    /// doc didn't advertise `end_session_endpoint`, so we cleared the
    /// Stackpit session but the IdP session is still live). Any other value
    /// is ignored.
    logout: Option<String>,
}

pub fn render_login(
    error: Option<String>,
    oauth_enabled: bool,
    status: StatusCode,
) -> axum::response::Response {
    let tmpl = LoginTemplate {
        error,
        oauth_enabled,
        info: None,
    };
    match askama::Template::render(&tmpl) {
        Ok(html) => (status, axum::response::Html(html)).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "render error").into_response(),
    }
}

/// Map a `?logout=` value to an info banner. Only `local` is meaningful
/// today; anything else returns `None` so we never echo attacker-controlled
/// strings into rendered HTML.
fn logout_message(code: &str) -> Option<String> {
    match code {
        "local" => Some(
            "Signed out of Stackpit. Your identity provider session was not ended -- sign \
             out there separately if needed."
                .into(),
        ),
        _ => None,
    }
}

/// Translate an OAuth error code into a user-readable message. Unknown codes
/// fall through to a deliberately generic line so we don't echo arbitrary
/// strings into rendered HTML; the original code lands in server logs at
/// `warn` so support can still trace it.
fn error_message(code: &str) -> String {
    match code {
        "state_mismatch" => {
            "Your sign-in session was tampered with or expired. Please try again.".into()
        }
        "flow_cookie_missing" | "flow_cookie_mismatch" => {
            "Your sign-in session was tampered with or expired. Please try again.".into()
        }
        "session_expired" => "Your session expired. Please sign in again.".into(),
        "missing_code" | "missing_state" => {
            "Your identity provider returned an incomplete response. Please try again.".into()
        }
        "token_exchange_failed" => {
            "We couldn't complete sign-in with your identity provider. Please try again in a \
             moment."
                .into()
        }
        "provisioning_failed" => {
            "Your account couldn't be created. Contact your administrator.".into()
        }
        "email_conflict" => {
            "An account with this email already exists. Contact your administrator.".into()
        }
        "session_unavailable" => {
            "Sign-in is temporarily unavailable. Please try again in a moment.".into()
        }
        "encryption_unconfigured" => {
            "Sign-in is misconfigured on this deployment. Contact your administrator.".into()
        }
        other => {
            // Unknown codes are almost always a sign that a new error path was
            // added without updating the mapping. Log so support can trace it;
            // render a generic message so we never echo attacker-controlled
            // strings into the HTML even though the redirect path is internal.
            tracing::warn!(
                target: "stackpit::auth",
                code = %other,
                "login redirect carried unknown error code; rendering generic message",
            );
            "Sign-in failed. Please try again.".into()
        }
    }
}

pub async fn login_form(
    State(state): State<AppState>,
    Query(q): Query<LoginQuery>,
) -> impl IntoResponse {
    let oauth_enabled = state.oidc.is_some();
    let error = q.error.as_deref().map(error_message);
    let info = q.logout.as_deref().and_then(logout_message);
    let tmpl = LoginTemplate {
        error,
        oauth_enabled,
        info,
    };
    let status = StatusCode::OK;
    match askama::Template::render(&tmpl) {
        Ok(html) => (status, axum::response::Html(html)).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "render error").into_response(),
    }
}

pub async fn handle_login(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    let token = form.token.trim().to_string();
    let oauth_enabled = state.oidc.is_some();

    // No admin_token set? Auth is effectively disabled -- let them through.
    let expected = match &state.config.server.admin_token {
        Some(t) => t,
        None => {
            return axum::response::Redirect::to("/web/projects/").into_response();
        }
    };

    if subtle::ConstantTimeEq::ct_eq(token.as_bytes(), expected.expose_secret().as_bytes()).into() {
        let secure = state.config.server.cookies_should_be_secure();
        let secure_flag = if secure { "; Secure" } else { "" };
        let name = admin_cookie_name(secure);
        let hashed = crate::middleware::hash_token_for_cookie(&token);
        let cookie = format!("{name}={hashed}; Path=/; SameSite=Strict; HttpOnly{secure_flag}");
        let mut resp = axum::response::Redirect::to("/web/projects/").into_response();
        if let Ok(val) = cookie.parse() {
            resp.headers_mut().insert("set-cookie", val);
        }
        resp
    } else {
        render_login(
            Some("Invalid token".to_string()),
            oauth_enabled,
            StatusCode::UNAUTHORIZED,
        )
    }
}

#[derive(Deserialize)]
pub struct LoginForm {
    token: String,
}

pub async fn handle_logout(State(state): State<AppState>) -> impl IntoResponse {
    let secure = state.config.server.cookies_should_be_secure();
    let secure_flag = if secure { "; Secure" } else { "" };
    let name = admin_cookie_name(secure);
    let cookie = format!("{name}=; Path=/; SameSite=Strict; HttpOnly; Max-Age=0{secure_flag}");
    let mut resp = axum::response::Redirect::to("/web/login").into_response();
    if let Ok(val) = cookie.parse() {
        resp.headers_mut().insert("set-cookie", val);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every known error code emitted by `src/html/auth.rs` must map to a
    /// non-default message (i.e. *not* the generic fallback). Catalogue is
    /// the set of `login_error("...")` and `finish_with_error(..., "...")`
    /// call sites in auth.rs as of writing.
    #[test]
    fn known_codes_have_specific_messages() {
        let known = [
            "state_mismatch",
            "flow_cookie_missing",
            "flow_cookie_mismatch",
            "session_expired",
            "missing_code",
            "missing_state",
            "token_exchange_failed",
            "provisioning_failed",
            "email_conflict",
            "session_unavailable",
            "encryption_unconfigured",
        ];
        let generic = error_message("__definitely_unknown_code__");
        for code in known {
            let msg = error_message(code);
            assert_ne!(
                msg, generic,
                "code `{code}` maps to the generic fallback; add a specific message"
            );
            assert!(
                !msg.contains(code),
                "code `{code}` leaks the raw identifier into the rendered message: {msg}"
            );
        }
    }

    #[test]
    fn logout_local_renders_info_banner() {
        let msg = super::logout_message("local").expect("local must map to a banner");
        assert!(
            msg.contains("Stackpit"),
            "info banner should mention Stackpit: {msg}"
        );
        assert!(
            msg.contains("identity provider"),
            "info banner should explain the IdP session was not ended: {msg}"
        );
    }

    #[test]
    fn logout_unknown_codes_render_nothing() {
        // Attacker-controlled / unknown values must not echo through.
        assert!(super::logout_message("").is_none());
        assert!(super::logout_message("remote").is_none());
        assert!(super::logout_message("<script>").is_none());
    }

    #[test]
    fn unknown_code_falls_back_to_generic_message() {
        let msg = error_message("not_a_real_code_xyz");
        assert!(
            !msg.contains("not_a_real_code_xyz"),
            "unknown codes must not echo into the message: {msg}"
        );
        assert!(
            msg.starts_with("Sign-in failed"),
            "unknown codes must use the generic fallback, got: {msg}"
        );
    }
}
