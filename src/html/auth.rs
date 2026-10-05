//! Browser OAuth flow: `/web/auth/{login,callback,logout,backchannel-logout}`.
//!
//! The web surface is a confidential OAuth2 client (BFF pattern). The
//! browser holds only an opaque `sp_grant` handle; the access + refresh
//! tokens live server-side in [`crate::oidc::grants::oidc_grants`],
//! encrypted with the master key.
//!
//! Pre-auth state (state / nonce / PKCE verifier) travels in a separate
//! short-lived encrypted cookie (`sp_login`) -- no tower-sessions row.

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use oidc_relying_party::auth_response::{check_authorization_response_iss, AuthorizationErrorCode};
use serde::Deserialize;

use crate::oidc::cookies::{
    append_set_cookie, build_grant_cookie, build_login_cookie, clear_login_cookie,
    login_cookie_name,
};
use crate::oidc::grants::{self, NewGrant};
use crate::oidc::login_state::{self, LoginState};
use crate::oidc::logout;
use crate::queries::users;
use crate::server::AppState;
use stackpit_auth::read_cookie;

/// `GET /web/auth/login` -- generate state/nonce/PKCE, stash in encrypted
/// cookie, redirect to Hydra.
pub async fn login(State(state): State<AppState>) -> Response {
    let Some(oidc) = state.oidc.client() else {
        // Not configured, or discovery hasn't landed yet - the admin-token form still works.
        return Redirect::to("/web/login").into_response();
    };
    let Some(encryptor) = state.encryptor.as_ref() else {
        // server.rs enforces encryptor-when-OAuth at startup; defense in depth
        tracing::error!("OAuth enabled but no encryptor configured");
        return login_error("encryption_unconfigured");
    };

    let start = match oidc.start_login() {
        Ok(start) => start,
        Err(e) => {
            tracing::error!("building the authorization request failed: {e:#}");
            return login_error("session_unavailable");
        }
    };
    let packed = match login_state::pack(
        encryptor,
        &LoginState::new(start.state.clone(), start.nonce, start.pkce_verifier),
    ) {
        Some(s) => s,
        None => {
            tracing::error!("encrypting login state failed");
            return login_error("session_unavailable");
        }
    };

    let mut resp = Redirect::to(&start.auth_url).into_response();
    append_set_cookie(
        &mut resp,
        build_login_cookie(&packed, state.config.server.cookies_should_be_secure()),
    );
    resp
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    /// RFC 9207 authorization-server issuer identifier.
    iss: Option<String>,
}

/// `GET /web/auth/callback` -- finish the auth-code flow, issue a grant.
pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let Some(oidc) = state.oidc.client() else {
        return Redirect::to("/web/login").into_response();
    };
    let Some(encryptor) = state.encryptor.as_ref() else {
        return finish_with_error(&state, "encryption_unconfigured");
    };

    // RFC 9207 §2.4: checked on success and error responses alike (mix-up defence).
    if check_authorization_response_iss(oidc.discovery(), q.iss.as_deref()).is_err() {
        tracing::warn!(iss = ?q.iss, "OAuth callback iss does not match the configured issuer");
        return finish_with_error(&state, "issuer_mismatch");
    }

    if let Some(err) = q.error.as_deref() {
        tracing::warn!(
            "OAuth callback returned error: {err} ({:?})",
            q.error_description
        );
        return finish_with_error(&state, sanitize_oauth_error(err));
    }

    let Some(code) = q.code else {
        return finish_with_error(&state, "missing_code");
    };
    let Some(returned_state) = q.state else {
        return finish_with_error(&state, "missing_state");
    };

    // forged or expired cookies fail decryption: the GCM tag is the integrity check
    let login_cookie = login_cookie_name(state.config.server.cookies_should_be_secure());
    let Some(packed) = read_cookie(&headers, login_cookie) else {
        return finish_with_error(&state, "session_expired");
    };
    let Some(login_state) = login_state::unpack(encryptor, packed) else {
        return finish_with_error(&state, "session_expired");
    };

    if !constant_time_eq(returned_state.as_bytes(), login_state.state.as_bytes()) {
        return finish_with_error(&state, "state_mismatch");
    }

    let success = match oidc
        .finish_login(code, login_state.pkce_verifier, &login_state.nonce)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("OAuth callback finish failed: {e:#}");
            return finish_with_error(&state, "token_exchange_failed");
        }
    };

    let user = match users::upsert_from_oidc(
        &state.auth_pool,
        &success.claims.iss,
        &success.claims.sub,
        success.claims.email.as_deref(),
        success.claims.name.as_deref(),
    )
    .await
    {
        Ok(u) => u,
        Err(e) => {
            if is_email_conflict(&e) {
                tracing::warn!(
                    "refusing login for sub '{}': email already bound to another account",
                    success.claims.sub
                );
                return finish_with_error(&state, "email_conflict");
            }
            tracing::error!("user upsert failed for sub '{}': {e:#}", success.claims.sub);
            return finish_with_error(&state, "provisioning_failed");
        }
    };

    // The OIDC claim seeds the language only when the user has not set one.
    if let Some(loc) = success.claims.locale.as_deref() {
        if let Err(e) =
            users::set_preferred_language_if_unset(&state.auth_pool, user.user_id, loc).await
        {
            tracing::warn!(
                "failed to persist preferred_language for user {}: {e:#}",
                user.user_id
            );
        }
    }

    warn_orgs_claim_absent_once(success.claims.orgs.is_none());

    let recon = crate::orgs::reconcile::reconcile(
        &state.auth_pool,
        crate::orgs::reconcile::ReconcileInput {
            user_id: user.user_id,
            iss: &success.claims.iss,
            orgs: success.claims.orgs.as_deref(),
            orgs_truncated: success.claims.orgs_truncated,
        },
    )
    .await
    .unwrap_or_else(|e| {
        tracing::error!("reconcile failed: {e:#}");
        Default::default()
    });

    // cookie carries only the handle; tokens persist encrypted server-side
    let handle = match grants::insert(
        &state.auth_pool,
        encryptor,
        &NewGrant {
            user_id: user.user_id,
            iss: &success.claims.iss,
            sub: &success.claims.sub,
            sid: success.claims.sid.as_deref(),
            access_token: &success.access_token,
            access_exp: success.access_exp,
            refresh_token: success.refresh_token.as_deref(),
            refresh_exp: success.refresh_exp,
            id_token: &success.id_token,
            offline: success.offline,
        },
    )
    .await
    {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("grant insert failed: {e:#}");
            return finish_with_error(&state, "session_unavailable");
        }
    };

    let secure = state.config.server.cookies_should_be_secure();

    // Build provision cookie first so we know if it succeeds before deciding redirect.
    let provision_cookie = if !recon.provisionable.is_empty() {
        let ps = crate::html::provision::new_state(
            recon.provisionable,
            success.claims.iss.clone(),
            user.user_id,
        );
        match crate::html::provision::pack(encryptor, &ps) {
            Some(blob) => Some(crate::html::provision::build_provision_cookie(
                &blob, secure,
            )),
            None => {
                tracing::error!("provisionable orgs present but sp_provision cookie could not be built; skipping interstitial");
                None
            }
        }
    } else {
        None
    };

    let redirect_target = if provision_cookie.is_some() {
        "/web/provision"
    } else {
        "/web/"
    };
    let mut resp = Redirect::to(redirect_target).into_response();
    append_set_cookie(&mut resp, build_grant_cookie(&handle.to_hex(), secure));
    append_set_cookie(&mut resp, clear_login_cookie(secure));
    if let Some(cookie) = provision_cookie {
        append_set_cookie(&mut resp, cookie);
    }
    resp
}

/// `POST /web/auth/backchannel-logout` -- Hydra POSTs a signed logout token
/// when the user logs out elsewhere. Validate strictly, write the
/// revocation marker, eager-delete matching grants, record the jti, then
/// revoke the session-bound refresh tokens (§2.7, best-effort). Returns
/// 200 OK (also for a replay) or 400 with an empty body per the spec.
pub async fn backchannel_logout(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> Response {
    let Some(oidc) = state.oidc.client() else {
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    };

    let Some(token) = form_urlencoded::parse(body.as_ref())
        .find(|(k, _)| k == "logout_token")
        .map(|(_, v)| v.into_owned())
    else {
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    };

    let Some(claims) = logout::validate_logout_token(&oidc, &token).await else {
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    };

    // Cap revocation marker lifetime at the larger of the access- and
    // refresh-token ceilings; otherwise a long-lived refresh token can
    // outlast the marker and re-arm a revoked grant on its next replay.
    let ttl = logout::revocation_ttl_secs(
        state.config.auth.oauth.access_token_max_ttl_secs,
        state.config.auth.oauth.refresh_token_max_ttl_secs,
    );

    // Read before apply_logout deletes the rows.
    let revocable = select_revocable(&state, &claims).await;

    let applied = logout::apply_logout(
        &state.auth_pool,
        &claims.iss,
        claims.sub.as_deref(),
        claims.sid.as_deref(),
        &claims.jti,
        claims.iat,
        ttl,
    )
    .await;
    if matches!(applied, Ok(()) | Err(logout::LogoutApplyError::Replay)) {
        revoke_refresh_tokens(&state, &oidc, revocable).await;
    }

    match applied {
        Ok(()) => {
            let mut resp = axum::http::StatusCode::OK.into_response();
            resp.headers_mut().insert(
                axum::http::header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static("no-store"),
            );
            resp
        }
        Err(logout::LogoutApplyError::Replay) => {
            tracing::info!(jti = %claims.jti, "back-channel logout replay; already applied");
            axum::http::StatusCode::OK.into_response()
        }
        Err(logout::LogoutApplyError::Db(e)) => {
            tracing::error!(error = %e, "back-channel logout DB write failed");
            // 400 (not 5xx, not 200) so Hydra retries instead of marking delivered
            axum::http::StatusCode::BAD_REQUEST.into_response()
        }
    }
}

/// Session-bound refresh tokens of the grants the logout token names, by
/// `sid` when present, else by `sub` (same selection as `apply_logout`).
async fn select_revocable(
    state: &AppState,
    claims: &oidc_relying_party::logout_token::LogoutTokenClaims,
) -> Vec<grants::RevocableRefresh> {
    if state.encryptor.is_none() {
        return Vec::new();
    }
    let selected = if let Some(sid) = claims.sid.as_deref().filter(|s| !s.is_empty()) {
        grants::select_revocable_by_sid(&state.auth_pool, &claims.iss, sid).await
    } else if let Some(sub) = claims.sub.as_deref().filter(|s| !s.is_empty()) {
        grants::select_revocable_by_sub(&state.auth_pool, &claims.iss, sub).await
    } else {
        return Vec::new();
    };
    selected.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "back-channel logout: selecting refresh tokens to revoke failed");
        Vec::new()
    })
}

async fn revoke_refresh_tokens(
    state: &AppState,
    oidc: &crate::oidc::client::OidcClient,
    revocable: Vec<grants::RevocableRefresh>,
) {
    let Some(encryptor) = state.encryptor.as_ref() else {
        return;
    };
    for row in revocable {
        let Some(mut pt) =
            encryptor.decrypt_bytes_with_aad(&row.refresh_token_bc, &row.hashed_handle)
        else {
            tracing::warn!("back-channel logout: decrypting a refresh token copy failed");
            continue;
        };
        match std::str::from_utf8(&pt) {
            Ok(token) => oidc.revoke_refresh_token(token).await,
            Err(_) => tracing::warn!("back-channel logout: refresh token copy is not UTF-8"),
        }
        zeroize::Zeroize::zeroize(&mut pt);
    }
}

fn login_error(code: &str) -> Response {
    Redirect::to(&format!("/web/login?error={code}")).into_response()
}

/// `q.error` is attacker-controlled; anything unregistered collapses to a
/// fixed code so it never reaches the Location header (control characters
/// would panic HeaderValue conversion, and arbitrary values inject params).
fn sanitize_oauth_error(code: &str) -> &'static str {
    AuthorizationErrorCode::parse(code)
        .as_str()
        .unwrap_or("oauth_error")
}

fn finish_with_error(state: &AppState, code: &str) -> Response {
    let mut resp = login_error(code);
    append_set_cookie(
        &mut resp,
        clear_login_cookie(state.config.server.cookies_should_be_secure()),
    );
    resp
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

/// Emits the missing-orgs-claim warning exactly once per process lifetime.
fn warn_orgs_claim_absent_once(absent: bool) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static WARNED: AtomicBool = AtomicBool::new(false);
    if absent && !WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            "OIDC id_token carried no `orgs` claim; org reconciliation disabled. \
             Is the `orgs` scope registered on this client in Hydra?"
        );
    }
}

/// Detect email unique constraint violation across SQLite + Postgres.
fn is_email_conflict(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(sqlx::Error::Database(db_err)) = cause.downcast_ref::<sqlx::Error>() {
            let msg = db_err.message();
            if msg.contains("idx_users_email_unique") || msg.contains("users.email") {
                return true;
            }
            if db_err.code().as_deref() == Some("23505") {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{login_error, sanitize_oauth_error};

    #[test]
    fn hostile_oauth_error_collapses_to_generic_and_builds_response() {
        let long_garbage = "x".repeat(8192);
        for hostile in [
            "evil\r\nSet-Cookie: pwned=1",
            "a\u{0}b",
            "access_denied&admin=1",
            "<script>alert(1)</script>",
            "",
            long_garbage.as_str(),
        ] {
            let code = sanitize_oauth_error(hostile);
            assert_eq!(code, "oauth_error", "hostile input must not pass through");
            // Building the redirect must not panic on HeaderValue conversion.
            let resp = login_error(code);
            assert_eq!(resp.status(), axum::http::StatusCode::SEE_OTHER);
            assert_eq!(
                resp.headers().get(axum::http::header::LOCATION).unwrap(),
                "/web/login?error=oauth_error"
            );
        }
    }

    #[cfg(feature = "sqlite")]
    async fn callback_location_without_iss(iss_parameter_supported: bool) -> String {
        use axum::body::Body;
        use axum::http::Request;
        use oidc_relying_party::jwks::JwksCacheConfig;
        use tower::ServiceExt;

        let mut oidc = crate::oidc::client::OidcClient::for_test(
            "https://idp.example.com".to_string(),
            "stackpit-web".to_string(),
            stackpit_auth::JwksCache::new(
                reqwest::Client::new(),
                url::Url::parse("http://127.0.0.1:0/jwks").unwrap(),
                JwksCacheConfig::default(),
            ),
        );
        oidc.discovery_mut()
            .authorization_response_iss_parameter_supported = iss_parameter_supported;

        let pool = crate::db::open_test_pool().await;
        let (mut state, _chans) = crate::server::AppState::for_test(pool);
        state.oidc = crate::oidc::discovery::OidcSlot::ready(crate::oidc::discovery::OidcReady {
            client: std::sync::Arc::new(oidc),
        });
        state.encryptor = Some(std::sync::Arc::new(
            crate::util::crypto::SecretEncryptor::from_config_or_env(Some(
                &secrecy::SecretString::from("11".repeat(32)),
            ))
            .unwrap()
            .unwrap(),
        ));
        let app = axum::Router::new()
            .route("/cb", axum::routing::get(super::callback))
            .with_state(state);
        let res = app
            .oneshot(
                Request::get("/cb?code=c&state=s")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        res.headers()
            .get(axum::http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn callback_without_iss_rejected_when_the_op_advertises_it() {
        assert_eq!(
            callback_location_without_iss(true).await,
            "/web/login?error=issuer_mismatch"
        );
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn callback_without_iss_accepted_when_the_op_does_not_advertise_it() {
        // Passes the issuer check and stops at the missing login cookie.
        assert_eq!(
            callback_location_without_iss(false).await,
            "/web/login?error=session_expired"
        );
    }

    /// Back-channel logout for one `sid` against a fake IdP whose revocation
    /// endpoint answers `revoke_status` and must be hit `expected_revokes` times.
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    async fn backchannel_logout_revokes(
        offline: bool,
        revoke_status: usize,
        expected_revokes: usize,
    ) {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use oidc_relying_party::algorithms::SigningAlgorithm;
        use oidc_relying_party::jwks::JwksCacheConfig;
        use oidc_relying_party::test_support::{
            jwks, logout_token_claims, rsa_key, sign_logout_token, FakeIdp, REVOCATION_PATH,
        };
        use tower::ServiceExt;

        const AUD: &str = "stackpit-web";
        let key = rsa_key();
        let mut idp = FakeIdp::start(
            &[SigningAlgorithm::Rs256],
            &jwks(std::slice::from_ref(&key)),
            serde_json::json!({}),
        )
        .await;
        let issuer = idp.issuer();
        let revoke = idp
            .server()
            .mock("POST", REVOCATION_PATH)
            .match_body("token=rt-1&token_type_hint=refresh_token")
            .with_status(revoke_status)
            .expect(expected_revokes)
            .create_async()
            .await;

        let cache = stackpit_auth::JwksCache::new(
            reqwest::Client::new(),
            url::Url::parse(&idp.jwks_url()).unwrap(),
            JwksCacheConfig::default(),
        );
        cache
            .prime_raw(&jwks(std::slice::from_ref(&key)).to_string())
            .unwrap();
        let mut oidc =
            crate::oidc::client::OidcClient::for_test(issuer.clone(), AUD.to_string(), cache);
        oidc.discovery_mut().revocation_endpoint =
            Some(url::Url::parse(&format!("{}{REVOCATION_PATH}", idp.url())).unwrap());

        let pool = crate::queries::test_helpers::open_test_db().await;
        let enc = std::sync::Arc::new(
            crate::util::crypto::SecretEncryptor::from_config_or_env(Some(
                &secrecy::SecretString::from("11".repeat(32)),
            ))
            .unwrap()
            .unwrap(),
        );
        let user = crate::queries::users::upsert_from_oidc(&pool, &issuer, "alice", None, None)
            .await
            .unwrap();
        let handle = crate::oidc::grants::insert(
            &pool,
            &enc,
            &crate::oidc::grants::NewGrant {
                user_id: user.user_id,
                iss: &issuer,
                sub: "alice",
                sid: Some("sid-1"),
                access_token: "at",
                access_exp: chrono::Utc::now().timestamp() + 3600,
                refresh_token: Some("rt-1"),
                refresh_exp: None,
                id_token: "it",
                offline,
            },
        )
        .await
        .unwrap();

        let (mut state, _chans) = crate::server::AppState::for_test(pool.clone());
        state.oidc = crate::oidc::discovery::OidcSlot::ready(crate::oidc::discovery::OidcReady {
            client: std::sync::Arc::new(oidc),
        });
        state.encryptor = Some(enc.clone());
        let app = axum::Router::new()
            .route("/bc", axum::routing::post(super::backchannel_logout))
            .with_state(state);
        let token = sign_logout_token(
            &key,
            Some(key.kid()),
            Some("logout+jwt"),
            &logout_token_claims(&issuer, AUD, Some("alice"), Some("sid-1")),
        );
        let res = app
            .oneshot(
                Request::post("/bc")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(format!("logout_token={token}")))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        assert!(crate::oidc::grants::load(&pool, &enc, &handle)
            .await
            .unwrap()
            .is_none());
        revoke.assert_async().await;
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[tokio::test]
    async fn backchannel_logout_revokes_a_session_bound_refresh_token_once() {
        backchannel_logout_revokes(false, 200, 1).await;
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[tokio::test]
    async fn backchannel_logout_leaves_an_offline_access_refresh_token_alone() {
        backchannel_logout_revokes(true, 200, 0).await;
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[tokio::test]
    async fn backchannel_logout_answers_200_when_revocation_fails() {
        backchannel_logout_revokes(false, 500, 1).await;
    }

    #[test]
    fn known_oauth_error_codes_pass_through() {
        for known in ["access_denied", "server_error", "login_required"] {
            assert_eq!(sanitize_oauth_error(known), known);
        }
    }
}
