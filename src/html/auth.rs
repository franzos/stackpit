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
use serde::Deserialize;

use crate::oidc::cookies::{
    append_set_cookie, build_grant_cookie, build_login_cookie, clear_login_cookie, LOGIN_COOKIE,
};
use crate::oidc::grants::{self, NewGrant};
use crate::oidc::login_state::{self, LoginState};
use crate::oidc::{logout, revocations};
use crate::queries::users;
use crate::server::AppState;
use stackpit_auth::read_cookie;

/// `GET /web/auth/login` -- generate state/nonce/PKCE, stash in encrypted
/// cookie, redirect to Hydra.
pub async fn login(State(state): State<AppState>) -> Response {
    let Some(oidc) = state.oidc.as_ref() else {
        // OAuth not configured -- admin-token form is still there.
        return Redirect::to("/web/login").into_response();
    };
    let Some(encryptor) = state.encryptor.as_ref() else {
        // Encryption is required when OAuth is enabled; server.rs enforces
        // this at startup. Defense in depth -- if we ever drift, fail.
        tracing::error!("OAuth enabled but no encryptor configured");
        return login_error("encryption_unconfigured");
    };

    let start = oidc.start_login().await;
    let packed = match login_state::pack(
        encryptor,
        &LoginState {
            state: start.state.clone(),
            nonce: start.nonce,
            pkce_verifier: start.pkce_verifier,
        },
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
}

/// `GET /web/auth/callback` -- finish the auth-code flow, issue a grant.
pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let Some(oidc) = state.oidc.as_ref() else {
        return Redirect::to("/web/login").into_response();
    };
    let Some(encryptor) = state.encryptor.as_ref() else {
        return finish_with_error(&state, "encryption_unconfigured");
    };

    if let Some(err) = q.error.as_deref() {
        tracing::warn!(
            "OAuth callback returned error: {err} ({:?})",
            q.error_description
        );
        return finish_with_error(&state, err);
    }

    let Some(code) = q.code else {
        return finish_with_error(&state, "missing_code");
    };
    let Some(returned_state) = q.state else {
        return finish_with_error(&state, "missing_state");
    };

    // Read + decrypt the pre-auth cookie. Forged or expired cookies fail
    // decryption -- the GCM tag is the integrity check.
    let Some(packed) = read_cookie(&headers, LOGIN_COOKIE) else {
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
        &state.pool,
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

    // Persist tokens (encrypted) under a fresh handle. The cookie carries
    // only the handle from this point forward.
    let handle = match grants::insert(
        &state.pool,
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
    let mut resp = Redirect::to("/web/").into_response();
    append_set_cookie(&mut resp, build_grant_cookie(&handle.to_hex(), secure));
    // Clear the now-consumed pre-auth cookie.
    append_set_cookie(&mut resp, clear_login_cookie(secure));
    resp
}

/// `POST /web/auth/logout` -- destroy local grant, redirect to Hydra's
/// `end_session_endpoint` (RP-initiated logout) so the IdP session also
/// goes away.
pub async fn logout_handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let secure = state.config.server.cookies_should_be_secure();

    // Best-effort local cleanup: delete the grant row + clear the cookie.
    let id_token_hint = match state.encryptor.as_ref() {
        Some(enc) => {
            match grants::resolve_from_headers(&headers, secure, enc, &state.pool).await {
                Some(record) => {
                    // GrantRecord's `Drop` zeroizes tokens -- clone, don't move.
                    let id_token = record.id_token.clone();
                    grants::forget(&state.pool, &record.handle).await;
                    id_token
                }
                None => None,
            }
        }
        None => None,
    };

    // RP-initiated logout: only if the IdP advertises an end_session_endpoint
    // AND we have an id_token to use as hint. Falls back to local-only logout,
    // signalled to the login page via `?logout=local` so the user gets an
    // info banner ("we cleared *our* session, not the IdP's").
    let target = match (
        state.oidc.as_ref().and_then(|o| o.end_session_endpoint()),
        id_token_hint.as_deref(),
    ) {
        (Some(endpoint), Some(hint)) => {
            let post = state.config.auth.oauth.post_logout_redirect_uri.as_deref();
            logout::build_end_session_url(endpoint, hint, post)
        }
        _ => "/web/login?logout=local".to_string(),
    };

    let mut resp = Redirect::to(&target).into_response();
    append_set_cookie(&mut resp, crate::oidc::cookies::clear_grant_cookie(secure));
    resp
}

/// `POST /web/auth/backchannel-logout` -- Hydra POSTs a signed logout token
/// when the user logs out elsewhere. Validate strictly, dedupe by jti,
/// write revocation marker, eager-delete matching grants. Returns 200 OK
/// or 400 with an empty body per the spec.
pub async fn backchannel_logout(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> Response {
    let Some(oidc) = state.oidc.as_ref() else {
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    };

    // Parse application/x-www-form-urlencoded body for `logout_token=...`.
    let Some(token) = form_urlencoded::parse(body.as_ref())
        .find(|(k, _)| k == "logout_token")
        .map(|(_, v)| v.into_owned())
    else {
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    };

    let validation = logout::validate_logout_token(oidc, &token).await;
    let logout::LogoutValidation::Ok {
        iss,
        sub,
        sid,
        jti,
        iat,
    } = validation
    else {
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    };

    // Cap revocation marker lifetime at the larger of the access- and
    // refresh-token ceilings; otherwise a long-lived refresh token can
    // outlast the marker and re-arm a revoked grant on its next replay.
    let ttl = logout::revocation_ttl_secs(
        state.config.auth.oauth.access_token_max_ttl_secs,
        state.config.auth.oauth.refresh_token_max_ttl_secs,
    );

    match logout::apply_logout(
        &state.pool,
        &iss,
        sub.as_deref(),
        sid.as_deref(),
        &jti,
        iat,
        ttl,
    )
    .await
    {
        Ok(()) => {
            let mut resp = axum::http::StatusCode::OK.into_response();
            resp.headers_mut().insert(
                axum::http::header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static("no-store"),
            );
            resp
        }
        Err(logout::LogoutApplyError::Replay) => {
            tracing::warn!(jti = %jti, "back-channel logout replay rejected");
            axum::http::StatusCode::BAD_REQUEST.into_response()
        }
        Err(logout::LogoutApplyError::Db(e)) => {
            tracing::error!(error = %e, "back-channel logout DB write failed");
            // Per spec we don't 5xx -- but we shouldn't silently 200 either,
            // since Hydra would consider the logout delivered. Returning 400
            // makes Hydra retry per the integration guide's retry policy.
            axum::http::StatusCode::BAD_REQUEST.into_response()
        }
    }
}

/// Self-service "log out everywhere": writes a sub-scoped revocation marker
/// + deletes every grant for this user. Exposed for future settings UI.
#[allow(dead_code)]
pub async fn revoke_all_for_user(
    pool: &crate::db::DbPool,
    iss: &str,
    sub: &str,
    ttl_secs: i64,
) -> anyhow::Result<()> {
    let expires_at = chrono::Utc::now().timestamp() + ttl_secs;
    revocations::insert_sub(pool, iss, sub, expires_at).await?;
    grants::delete_by_sub(pool, iss, sub).await?;
    Ok(())
}

fn login_error(code: &str) -> Response {
    Redirect::to(&format!("/web/login?error={code}")).into_response()
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
