use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use oidc_relying_party::auth_response::AuthorizationErrorCode;
use oidc_relying_party::end_session::{build_end_session_url, EndSessionRequest};
use secrecy::ExposeSecret;
use serde::Deserialize;

use crate::html::chrome::Localized;
use crate::html::utils::Chrome;
use crate::locale::LanguageIdentifier;
use crate::oidc::cookies::{append_set_cookie, clear_grant_cookie_all_variants};
use crate::oidc::grants;
use crate::server::AppState;

pub const ADMIN_COOKIE: &str = "stackpit_token";
pub const ADMIN_COOKIE_HOST: &str = "__Host-stackpit_token";

pub const CSRF_SALT_COOKIE: &str = "stackpit_csrf_salt";
pub const CSRF_SALT_COOKIE_HOST: &str = "__Host-stackpit_csrf_salt";

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

/// Salt cookie name, mirroring [`admin_cookie_name`]'s `__Host-` posture.
pub fn csrf_salt_cookie_name(secure: bool) -> &'static str {
    if secure {
        CSRF_SALT_COOKIE_HOST
    } else {
        CSRF_SALT_COOKIE
    }
}

/// Per-session CSRF salt cookie. Same flags as the admin token cookie so it
/// rides along for the whole admin session; the CSRF derivation folds it in
/// so an attacker who only knows `admin_token` can't precompute the token.
pub fn build_csrf_salt_cookie(salt: &str, secure: bool) -> String {
    let name = csrf_salt_cookie_name(secure);
    let secure_flag = if secure { "; Secure" } else { "" };
    format!("{name}={salt}; Path=/; SameSite=Strict; HttpOnly{secure_flag}")
}

/// Clear all admin-token + CSRF-salt cookie variants. `__Host-` clears must
/// carry `Secure` to be accepted; the bare variants must not.
fn clear_session_cookies() -> [String; 4] {
    let clear = |name: &str, secure: bool| {
        let secure_flag = if secure { "; Secure" } else { "" };
        format!("{name}=; Path=/; SameSite=Strict; HttpOnly; Max-Age=0{secure_flag}")
    };
    [
        clear(ADMIN_COOKIE, false),
        clear(ADMIN_COOKIE_HOST, true),
        clear(CSRF_SALT_COOKIE, false),
        clear(CSRF_SALT_COOKIE_HOST, true),
    ]
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
    /// Resolved request locale. LoginTemplate is standalone (no PageChrome),
    /// so it carries its own locale and looks strings up directly.
    locale: LanguageIdentifier,
}

impl Localized for LoginTemplate {
    fn locale(&self) -> &LanguageIdentifier {
        &self.locale
    }
}

#[derive(askama::Template)]
#[template(path = "logout_continue.html")]
struct LogoutContinueTemplate {
    target: String,
    locale: LanguageIdentifier,
}

impl Localized for LogoutContinueTemplate {
    fn locale(&self) -> &LanguageIdentifier {
        &self.locale
    }
}

/// Hand the browser to the IdP's `end_session_endpoint` from a document rather
/// than a 303. Chrome and Safari check `form-action` on every hop of a form
/// submission's redirect chain, so a 303 out of the logout POST is blocked by
/// our `form-action 'self'` and the user sits on an unchanged page. A
/// declarative refresh is navigation type "other", which `form-action` never
/// covers, so the policy stays strict and any IdP's endpoint works.
fn continue_to_idp(target: &str, locale: LanguageIdentifier) -> axum::response::Response {
    if !url::Url::parse(target).is_ok_and(|u| matches!(u.scheme(), "http" | "https")) {
        tracing::error!(target: "stackpit::auth", "refusing a non-HTTP(S) end-session URL");
        return axum::response::Redirect::to("/web/login?logout=local").into_response();
    }
    let tmpl = LogoutContinueTemplate {
        target: target.to_string(),
        locale,
    };
    match askama::Template::render(&tmpl) {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "render error").into_response(),
    }
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
    locale: LanguageIdentifier,
) -> axum::response::Response {
    let tmpl = LoginTemplate {
        error,
        oauth_enabled,
        info: None,
        locale,
    };
    match askama::Template::render(&tmpl) {
        Ok(html) => (status, axum::response::Html(html)).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "render error").into_response(),
    }
}

/// Map a `?logout=` value to an info banner. Only `local` is meaningful
/// today; anything else returns `None` so we never echo attacker-controlled
/// strings into rendered HTML.
fn logout_message(locale: &LanguageIdentifier, code: &str) -> Option<String> {
    match code {
        "local" => Some(crate::i18n::lookup(locale, "login-logout-local")),
        _ => None,
    }
}

/// Translate an OAuth error code into a user-readable message. Unknown codes
/// fall through to a deliberately generic line so we don't echo arbitrary
/// strings into rendered HTML; the original code lands in server logs at
/// `warn` so support can still trace it.
fn error_message(locale: &LanguageIdentifier, code: &str) -> String {
    let key = match code {
        "state_mismatch" | "flow_cookie_missing" | "flow_cookie_mismatch" => {
            "login-error-state-mismatch"
        }
        "session_expired" => "login-error-session-expired",
        "missing_code" | "missing_state" => "login-error-missing-response",
        "token_exchange_failed" | "issuer_mismatch" => "login-error-token-exchange",
        "provisioning_failed" => "login-error-provisioning",
        "email_conflict" => "login-error-email-conflict",
        "session_unavailable" => "login-error-session-unavailable",
        "encryption_unconfigured" => "login-error-encryption",
        other => match AuthorizationErrorCode::parse(other) {
            AuthorizationErrorCode::AccessDenied => "login-error-access-denied",
            AuthorizationErrorCode::LoginRequired
            | AuthorizationErrorCode::InteractionRequired
            | AuthorizationErrorCode::ConsentRequired => "login-error-interaction-required",
            _ => {
                // Log unknown codes (usually a new error path missing here) but
                // render a generic message so we never echo arbitrary input into HTML.
                tracing::warn!(
                    target: "stackpit::auth",
                    code = %other,
                    "login redirect carried unknown error code; rendering generic message",
                );
                "login-error-generic"
            }
        },
    };
    crate::i18n::lookup(locale, key)
}

pub async fn login_form(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    Query(q): Query<LoginQuery>,
) -> impl IntoResponse {
    let locale = chrome.locale;
    // Ready, not merely configured: the SSO button shows only when it would reach the IdP.
    let oauth_enabled = state.oidc.is_ready();
    let error = q.error.as_deref().map(|c| error_message(&locale, c));
    let info = q.logout.as_deref().and_then(|c| logout_message(&locale, c));
    let tmpl = LoginTemplate {
        error,
        oauth_enabled,
        info,
        locale,
    };
    let status = StatusCode::OK;
    match askama::Template::render(&tmpl) {
        Ok(html) => (status, axum::response::Html(html)).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "render error").into_response(),
    }
}

pub async fn handle_login(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    let token = form.token.trim().to_string();
    let oauth_enabled = state.oidc.is_ready();

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
        // Per-login random handle, validated against the server-side store;
        // the cookie never carries anything derived from admin_token.
        let handle = crate::util::crypto::random_hex::<32>();
        let ttl = stackpit_auth::ADMIN_SESSION_TTL_SECS;
        state
            .admin_sessions
            .insert(&handle, std::time::Duration::from_secs(ttl));
        let cookie = format!(
            "{name}={handle}; Path=/; SameSite=Strict; HttpOnly; Max-Age={ttl}{secure_flag}"
        );
        // Fresh per-login salt so the CSRF token isn't a fixed function of admin_token.
        let salt = crate::util::crypto::random_hex::<32>();
        let salt_cookie = build_csrf_salt_cookie(&salt, secure);
        let mut resp = axum::response::Redirect::to("/web/projects/").into_response();
        if let Ok(val) = cookie.parse() {
            resp.headers_mut().append("set-cookie", val);
        }
        if let Ok(val) = salt_cookie.parse() {
            resp.headers_mut().append("set-cookie", val);
        }
        resp
    } else {
        let msg = crate::i18n::lookup(&chrome.locale, "login-error-invalid-token");
        render_login(
            Some(msg),
            oauth_enabled,
            StatusCode::UNAUTHORIZED,
            chrome.locale,
        )
    }
}

#[derive(Deserialize)]
pub struct LoginForm {
    token: String,
}

/// Universal logout for both admin-token and OIDC (SSO) sessions. Clears the
/// admin cookie + CSRF salt, and -- when OAuth is enabled -- tears down the
/// server-side grant and runs RP-initiated logout against the IdP.
pub async fn handle_logout(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    headers: HeaderMap,
) -> impl IntoResponse {
    let secure = state.config.server.cookies_should_be_secure();

    // Revoke server-side so the handle dies even if the cookie clear is lost.
    for name in [ADMIN_COOKIE, ADMIN_COOKIE_HOST] {
        if let Some(handle) = stackpit_auth::read_cookie(&headers, name) {
            state.admin_sessions.revoke(handle);
        }
    }

    let mut had_grant = false;
    let mut id_token_hint = None;
    // Configured, not ready: a grant outlives a provider outage and must still be cleared.
    if let (true, Some(encryptor)) = (
        state.config.auth.oauth.is_enabled(),
        state.encryptor.as_ref(),
    ) {
        // auth_pool to match the middleware grant branch.
        if let Some(record) =
            grants::resolve_from_headers(&headers, secure, encryptor, &state.auth_pool).await
        {
            had_grant = true;
            // GrantRecord's Drop zeroizes tokens; clone before forgetting.
            id_token_hint = record.id_token.clone();
            // Best-effort RFC 7009: forgetting the row only stops *us*
            // honouring the refresh token. Until the IdP revokes it, anything
            // that lifted a copy can still redeem it for a fresh access token.
            if let (Some(refresh), Some(client)) =
                (record.refresh_token.as_deref(), state.oidc.client())
            {
                client.revoke_refresh_token(refresh).await;
            }
            grants::forget(&state.auth_pool, &record.handle).await;
        }
    }

    // RP-initiated logout if the IdP advertises end_session_endpoint and we have
    // an id_token hint; else local-only banner for OIDC sessions; else plain.
    // `post_logout_redirect_uri` is operator config, validated at startup; no
    // request data reaches it, and the IdP echoes it to the browser.
    let post_logout = state.config.auth.oauth.post_logout_redirect_uri.as_deref();
    let oidc_client = state.oidc.client();
    let end_session = oidc_client
        .as_deref()
        .zip(id_token_hint.as_deref())
        .and_then(|(client, hint)| {
            build_end_session_url(
                client.discovery(),
                &EndSessionRequest {
                    id_token_hint: Some(hint),
                    client_id: Some(client.client_id()),
                    post_logout_redirect_uri: post_logout,
                    state: None,
                },
            )
        });
    let mut resp = match end_session {
        Some(url) => continue_to_idp(url.as_str(), chrome.locale),
        _ if had_grant => axum::response::Redirect::to("/web/login?logout=local").into_response(),
        _ => axum::response::Redirect::to("/web/login").into_response(),
    };
    // Clear both name variants of every session cookie so a stale
    // opposite-posture cookie can't linger and recreate the admin+OIDC overlap.
    for cookie in clear_session_cookies() {
        if let Ok(val) = cookie.parse() {
            resp.headers_mut().append("set-cookie", val);
        }
    }
    for val in clear_grant_cookie_all_variants() {
        append_set_cookie(&mut resp, val);
    }
    resp.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::default_locale;
    use askama::Template;
    use unic_langid::langid;

    #[tokio::test]
    async fn idp_logout_is_a_document_not_a_redirect() {
        let target = "https://id.example.com/oauth2/sessions/logout?id_token_hint=x&state=y";
        let resp = continue_to_idp(target, default_locale());
        // A 3xx would stay inside the form submission and trip `form-action 'self'`.
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains(r#"http-equiv="refresh""#));
        let decoded = html.replace("&#38;", "&").replace("&amp;", "&");
        assert!(decoded.contains(target), "{html}");
    }

    #[test]
    fn idp_logout_refuses_non_http_targets() {
        let resp = continue_to_idp("javascript:alert(1)", default_locale());
        assert!(resp.status().is_redirection());
    }

    /// Every known error code emitted by `src/html/auth.rs` must map to a
    /// non-default message (i.e. *not* the generic fallback). Catalogue is
    /// the set of `login_error("...")` and `finish_with_error(..., "...")`
    /// call sites in auth.rs as of writing.
    #[test]
    fn known_codes_have_specific_messages() {
        let en = default_locale();
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
        let generic = error_message(&en, "__definitely_unknown_code__");
        for code in known {
            let msg = error_message(&en, code);
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
        let msg =
            super::logout_message(&default_locale(), "local").expect("local must map to a banner");
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
        let en = default_locale();
        assert!(super::logout_message(&en, "").is_none());
        assert!(super::logout_message(&en, "remote").is_none());
        assert!(super::logout_message(&en, "<script>").is_none());
    }

    // An IdP without `end_session_endpoint`: logout ends the Stackpit
    // session and says so, rather than failing.
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[tokio::test]
    async fn end_session_absent_logs_out_locally() {
        use crate::oidc::grants::{self, NewGrant};
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let pool = crate::queries::test_helpers::open_test_db().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "https://idp", "sub-lo", None, None)
            .await
            .unwrap();
        let enc = std::sync::Arc::new(
            crate::util::crypto::SecretEncryptor::from_config_or_env(Some(
                &secrecy::SecretString::from("11".repeat(32)),
            ))
            .unwrap()
            .unwrap(),
        );
        let handle = grants::insert(
            &pool,
            &enc,
            &NewGrant {
                user_id: u.user_id,
                iss: "https://idp",
                sub: "sub-lo",
                sid: None,
                access_token: "at",
                access_exp: 0,
                refresh_token: None,
                refresh_exp: None,
                id_token: "it",
                offline: false,
            },
        )
        .await
        .unwrap();

        let (mut state, _chans) = crate::server::AppState::for_test(pool.clone());
        let mut config = crate::config::Config::default();
        config.auth.oauth.issuer_url = Some("https://idp".to_string());
        config.auth.oauth.client_id = Some("stackpit".to_string());
        config.auth.oauth.client_secret = Some(secrecy::SecretString::from("s"));
        config.auth.oauth.redirect_uri = Some("https://sp/web/auth/callback".to_string());
        state.config = std::sync::Arc::new(config);
        state.encryptor = Some(enc.clone());
        state.oidc = crate::oidc::discovery::OidcSlot::ready(crate::oidc::discovery::OidcReady {
            client: std::sync::Arc::new(crate::oidc::client::OidcClient::for_test(
                "https://idp".to_string(),
                "stackpit".to_string(),
                stackpit_auth::JwksCache::new(
                    reqwest::Client::new(),
                    url::Url::parse("https://idp/jwks").unwrap(),
                    crate::oidc::client::jwks_cache_config(60),
                ),
            )),
        });
        let app = axum::Router::new()
            .route("/web/logout", axum::routing::post(super::handle_logout))
            .with_state(state);
        let res = app
            .oneshot(
                Request::post("/web/logout")
                    .header(
                        "cookie",
                        format!(
                            "{}={}",
                            crate::oidc::cookies::grant_cookie_name(false),
                            handle.to_hex()
                        ),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res.headers().get("location").and_then(|v| v.to_str().ok()),
            Some("/web/login?logout=local")
        );
        assert!(grants::load(&pool, &enc, &handle).await.unwrap().is_none());
    }

    #[test]
    fn callback_access_denied_shows_message() {
        let en = default_locale();
        let generic = error_message(&en, "not_a_real_code_xyz");
        for code in [
            "access_denied",
            "login_required",
            "consent_required",
            "interaction_required",
        ] {
            let msg = error_message(&en, code);
            assert_ne!(msg, generic, "{code} must get its own message");
            assert!(!msg.contains(code), "{code} must not show the raw code");
        }
    }

    #[test]
    fn unknown_code_falls_back_to_generic_message() {
        let msg = error_message(&default_locale(), "not_a_real_code_xyz");
        assert!(
            !msg.contains("not_a_real_code_xyz"),
            "unknown codes must not echo into the message: {msg}"
        );
        assert!(
            msg.starts_with("Sign-in failed"),
            "unknown codes must use the generic fallback, got: {msg}"
        );
    }

    #[test]
    fn login_renders_german_without_missing_keys() {
        let tmpl = LoginTemplate {
            error: None,
            oauth_enabled: true,
            info: None,
            locale: langid!("de"),
        };
        let html = tmpl.render().expect("login renders");
        assert!(
            html.contains(r#"lang="de""#),
            "expected the German language attribute on the standalone login page"
        );
        assert!(
            html.contains("Anmelden"),
            "expected the German sign-in label in the output"
        );
        assert!(
            !html.contains(crate::i18n::MISSING_PREFIX),
            "German login render leaked a missing localization key: {html}"
        );
    }
}
