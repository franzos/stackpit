//! Locale switch endpoints: a GET link target (`/web/lang/{code}`) for the
//! login/in-app switchers and a POST form handler (`/web/settings/language`)
//! that also persists the preference for real OIDC users.

use axum::extract::{Form, Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Extension;
use serde::Deserialize;
use stackpit_auth::AuthContext;

use crate::server::AppState;

const DEFAULT_NEXT: &str = "/web/projects/";
const SETTINGS_DEFAULTS: &str = "/web/settings/defaults/";

#[derive(Deserialize)]
pub struct LangQuery {
    next: Option<String>,
}

/// `GET /web/lang/{code}`: validate the locale, set the `sp_locale` cookie, and
/// 303 to a validated local `?next=` (or `/web/projects/`). Public + idempotent.
pub async fn get_lang(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Query(q): Query<LangQuery>,
) -> Response {
    let Some(accepted) = crate::locale::accept(&code) else {
        // Unsupported locale: bounce home without touching the cookie.
        return switch_response(DEFAULT_NEXT, None);
    };
    let target = q
        .next
        .as_deref()
        .and_then(safe_local_next)
        .unwrap_or_else(|| DEFAULT_NEXT.to_string());
    let secure = state.config.server.cookies_should_be_secure();
    let cookie = crate::locale::locale_cookie(&accepted.to_string(), secure);
    switch_response(&target, Some(&cookie))
}

#[derive(Deserialize)]
pub struct LanguageForm {
    lang: String,
}

/// `POST /web/settings/language`: set the cookie and, for real OIDC users,
/// persist `users.preferred_language`. CSRF is enforced by the web middleware.
pub async fn post_language(
    State(state): State<AppState>,
    opt_auth: Option<Extension<AuthContext>>,
    Form(form): Form<LanguageForm>,
) -> Response {
    let Some(accepted) = crate::locale::accept(&form.lang) else {
        return switch_response(SETTINGS_DEFAULTS, None);
    };
    let code = accepted.to_string();
    let secure = state.config.server.cookies_should_be_secure();
    let cookie = crate::locale::locale_cookie(&code, secure);

    // Persist per-account only for OIDC users; admin-token/no-auth get cookie only.
    if let Some(AuthContext::User { iss, sub, .. }) = opt_auth.as_ref().map(|e| &e.0) {
        match crate::queries::users::find_by_iss_sub(&state.pool, iss, sub).await {
            Ok(Some(user)) => {
                let write = crate::queries::users::set_preferred_language(
                    &state.auth_pool,
                    user.user_id,
                    Some(&code),
                )
                .await;
                if let Err(e) = write {
                    tracing::error!("set_preferred_language failed in post_language: {e:#}");
                }
            }
            Ok(None) => {}
            Err(e) => tracing::error!("find_by_iss_sub failed in post_language: {e:#}"),
        }
    }
    switch_response(SETTINGS_DEFAULTS, Some(&cookie))
}

/// A local `next` target is accepted only if it parses to a scheme/authority-free
/// URI whose path is under `/web/` and carries no `..` traversal segment.
fn safe_local_next(next: &str) -> Option<String> {
    if next.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return None;
    }
    if next.contains('\\') {
        return None;
    }
    let uri: axum::http::Uri = next.parse().ok()?;
    if uri.scheme().is_some() || uri.authority().is_some() {
        return None;
    }
    let path = uri.path();
    if !path.starts_with("/web/") {
        return None;
    }
    if path.split('/').any(|s| s == "..") {
        return None;
    }
    Some(next.to_string())
}

/// 303 to `target` with `Cache-Control: no-store` and, optionally, a Set-Cookie.
/// Header values are built fallibly; a malformed target falls back to home.
fn switch_response(target: &str, cookie: Option<&str>) -> Response {
    let mut resp = StatusCode::SEE_OTHER.into_response();
    let headers = resp.headers_mut();
    let location =
        HeaderValue::from_str(target).unwrap_or_else(|_| HeaderValue::from_static(DEFAULT_NEXT));
    headers.insert(header::LOCATION, location);
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(c) = cookie {
        if let Ok(v) = HeaderValue::from_str(c) {
            headers.insert(header::SET_COOKIE, v);
        }
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::safe_local_next;

    #[test]
    fn accepts_local_web_paths() {
        assert_eq!(
            safe_local_next("/web/projects/"),
            Some("/web/projects/".to_string())
        );
        assert_eq!(
            safe_local_next("/web/login"),
            Some("/web/login".to_string())
        );
    }

    #[test]
    fn rejects_hostile_targets() {
        assert_eq!(safe_local_next("//evil.com"), None);
        assert_eq!(safe_local_next("https://evil"), None);
        assert_eq!(safe_local_next("/web\\evil"), None);
        assert_eq!(safe_local_next("/web/../admin"), None);
        assert_eq!(safe_local_next("/etc"), None);
        assert_eq!(safe_local_next("/web/\tinject"), None);
        assert_eq!(safe_local_next(""), None);
    }
}
