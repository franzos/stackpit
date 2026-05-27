//! Browser auth gate: resolves the request identity into [`AuthContext`].
//!
//! Priority:
//! 1. **admin_token** (header or cookie) -- break-glass, never hits the IdP.
//! 2. **`sp_grant` cookie** -- look up the server-side grant, refresh the
//!    access token if it's close to expiring, introspect it via the
//!    `BearerGate` (which also checks revocations), inject `AuthContext`.
//! 3. Otherwise -- redirect to `/web/login` (or 401 JSON for API clients).
//!
//! Public paths (assets, OAuth callback, ingest API) skip the gate entirely.

use axum::body::Body;
use axum::http::{HeaderValue, Request};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use secrecy::ExposeSecret;
use stackpit_auth::axum_ext::middleware as auth_mw;
use stackpit_auth::{AuthContext, BearerAuthOutcome, PrincipalId};

use crate::middleware::{derive_admin_csrf_token, CsrfToken};
use crate::oidc::cookies::{append_set_cookie, clear_grant_cookie};
use crate::oidc::grants::{self, GrantHandle};
use crate::oidc::refresh::{self, RefreshOutcome};
use crate::server::AppState;

/// Refresh-ahead margin. Eager refresh costs one extra `/token` call.
const REFRESH_MARGIN_SECS: i64 = 60;

/// Public paths (includes OAuth callback + backchannel + ops health).
fn is_public_path(path: &str) -> bool {
    path == "/web/login"
        || path == "/health"
        || path.starts_with("/web/_assets/")
        || path.starts_with("/api/0/")
        || path.starts_with("/web/auth/")
}

pub async fn web_auth_middleware(
    axum::extract::State(state): axum::extract::State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();

    let admin_token = state.config.server.admin_token.as_ref();
    let oauth_enabled = state.oidc.is_some();
    let secure_cookies = state.config.server.cookies_should_be_secure();

    // Defense-in-depth: pass-through only when the ack flag is true;
    // config validation already enforces loopback + ack at startup.
    if admin_token.is_none() && !oauth_enabled {
        if state.config.server.no_auth_loopback_acknowledged {
            // CSRF still wants something to compare against; pass-through
            // auth means the token is a constant, not a secret.
            req.extensions_mut().insert(CsrfToken("noauth".to_string()));
            return next.run(req).await;
        }
        return unauthenticated_response(&req, secure_cookies);
    }

    if is_public_path(&path) {
        return next.run(req).await;
    }

    // 1. admin_token break-glass -- works even when Hydra is down.
    if let Some(expected) = admin_token {
        let admin_cookie = crate::html::login::admin_cookie_name(secure_cookies);
        if let Some(ctx) =
            auth_mw::resolve_admin(req.headers(), expected.expose_secret(), admin_cookie)
        {
            // Derived deterministically so admin sessions need no extra storage.
            let csrf = derive_admin_csrf_token(expected.expose_secret());
            req.extensions_mut().insert(ctx);
            req.extensions_mut().insert(CsrfToken(csrf));
            return next.run(req).await;
        }
    }

    // 2. sp_grant cookie -- BFF token vault lookup.
    if oauth_enabled {
        let (Some(oidc), Some(encryptor), Some(gate)) = (
            state.oidc.as_ref(),
            state.encryptor.as_ref(),
            state.web_bearer_gate.as_ref(),
        ) else {
            // Misconfiguration -- should have been caught at startup.
            return unauthenticated_response(&req, secure_cookies);
        };

        let Some(grant) = grants::resolve_from_headers(
            req.headers(),
            secure_cookies,
            encryptor,
            &state.auth_pool,
        )
        .await
        else {
            return unauthenticated_response(&req, secure_cookies);
        };
        let handle = grant.handle.clone();

        // Failures fall through to the existing token; the gate will reject if expired.
        let now = chrono::Utc::now().timestamp();
        let grant = if grant.should_refresh(now, REFRESH_MARGIN_SECS) {
            match refresh::refresh(&state.auth_pool, encryptor, oidc, &grant).await {
                Ok(RefreshOutcome::Refreshed(g)) => g,
                Ok(RefreshOutcome::InvalidGrant) => {
                    tracing::info!("refresh token rejected; forcing re-login");
                    grants::forget(&state.auth_pool, &handle).await;
                    return unauthenticated_response(&req, secure_cookies);
                }
                Ok(RefreshOutcome::Transient(msg)) => {
                    tracing::warn!(
                        "transient refresh failure ({msg}); falling back to existing access token"
                    );
                    grant
                }
                Err(e) => {
                    tracing::error!("refresh DB write failed: {e:#}");
                    grant
                }
            }
        } else {
            grant
        };

        match gate
            .authorize(
                Some(&grant.access_token),
                state.config.auth.oauth.web_required_scope.as_str(),
            )
            .await
        {
            BearerAuthOutcome::Ok(_) => {
                // Stable grant handle so audit logs correlate per browser session.
                let handle_uuid = handle_to_uuid(&grant.handle);
                // GrantRecord's `Drop` zeroizes tokens -- clone iss/sub instead of moving.
                let ctx = AuthContext::User {
                    iss: grant.iss.clone(),
                    sub: grant.sub.clone(),
                    principal_id: PrincipalId::Session(handle_uuid),
                };
                if let AuthContext::User { iss, sub, .. } = &ctx {
                    tracing::debug!(
                        target: "stackpit::auth",
                        auth_source = %ctx.source(),
                        iss = %iss,
                        sub = %sub,
                        principal_id = %handle_uuid,
                        "request authenticated",
                    );
                }
                let csrf = grant.csrf_token.clone();
                req.extensions_mut().insert(ctx);
                req.extensions_mut().insert(CsrfToken(csrf));
                return next.run(req).await;
            }
            _ => {
                // Revoked or expired beyond refresh; drop the grant.
                grants::forget(&state.auth_pool, &handle).await;
                return unauthenticated_response(&req, secure_cookies);
            }
        }
    }

    // 3. No identity.
    unauthenticated_response(&req, secure_cookies)
}

fn unauthenticated_response(req: &Request<Body>, secure_cookies: bool) -> Response {
    let mut resp = if auth_mw::wants_html(req.headers()) {
        Redirect::to("/web/login").into_response()
    } else {
        auth_mw::json_unauthorized()
    };
    append_set_cookie(&mut resp, clear_grant_cookie(secure_cookies));
    // Don't let HTML caches hold a stale login redirect.
    resp.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    resp
}

/// Audit UUID = `SHA-256(handle)[..16]` (hashed so logs don't leak the cookie secret), patched to v8 per RFC 9562.
fn handle_to_uuid(handle: &GrantHandle) -> uuid::Uuid {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(handle.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes)
}
