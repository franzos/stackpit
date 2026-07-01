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
        || path == "/web/auth/login"
        || path == "/web/auth/callback"
        || path == "/web/auth/backchannel-logout"
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
            req.extensions_mut().insert(crate::orgs::extractor::ActiveOrg { org_id: 1, role: None });
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
            // Same salt cookie feeds render (here) and verify (csrf_middleware
            // reads the CsrfToken we insert), so the tokens always match. A
            // logged-in admin missing the cookie gets a fresh salt set below.
            let salt_cookie = crate::html::login::csrf_salt_cookie_name(secure_cookies);
            let (salt, set_salt) =
                match crate::middleware::cookie::read_cookie(req.headers(), salt_cookie) {
                    Some(s) => (s.to_string(), false),
                    None => (crate::util::crypto::random_hex::<32>(), true),
                };
            let csrf = derive_admin_csrf_token(expected.expose_secret(), &salt);
            req.extensions_mut().insert(ctx);
            req.extensions_mut().insert(CsrfToken(csrf));
            let admin_org_id = state
                .encryptor
                .as_deref()
                .and_then(|enc| {
                    crate::middleware::cookie::read_cookie(req.headers(), crate::orgs::extractor::ACTIVE_ORG_COOKIE)
                        .and_then(|v| crate::orgs::extractor::unpack(enc, v))
                })
                .unwrap_or(1);
            req.extensions_mut()
                .insert(crate::orgs::extractor::ActiveOrg { org_id: admin_org_id, role: None });
            let mut resp = next.run(req).await;
            if set_salt {
                if let Ok(val) =
                    crate::html::login::build_csrf_salt_cookie(&salt, secure_cookies).parse()
                {
                    resp.headers_mut().append("set-cookie", val);
                }
            }
            return resp;
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

        // Logout must succeed even with an expired or revoked access token, so it
        // skips the bearer gate + eager refresh. Inject the per-grant CSRF token so
        // the synchronizer check passes; the handler does the actual teardown.
        if path == "/web/logout" {
            req.extensions_mut()
                .insert(CsrfToken(grant.csrf_token.clone()));
            return next.run(req).await;
        }

        // Failures fall through to the existing token; the gate will reject if expired.
        let now = chrono::Utc::now().timestamp();
        let cap = state.config.auth.oauth.session_max_ttl_secs;
        if session_expired(grant.created_at, now, cap) {
            tracing::info!("session absolute TTL exceeded; forcing re-login");
            grants::forget(&state.auth_pool, &handle).await;
            return unauthenticated_response(&req, secure_cookies);
        }
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
                let user_id = grant.user_id;
                req.extensions_mut().insert(ctx);
                req.extensions_mut().insert(CsrfToken(csrf));
                let active_org = resolve_session_active_org(
                    &state.auth_pool,
                    user_id,
                    req.headers(),
                    state.encryptor.as_deref(),
                )
                .await;
                req.extensions_mut().insert(active_org);
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

/// Load memberships and resolve which org is active for a browser session user.
async fn resolve_session_active_org(
    pool: &crate::db::DbPool,
    user_id: i64,
    headers: &axum::http::HeaderMap,
    encryptor: Option<&crate::util::crypto::SecretEncryptor>,
) -> crate::orgs::extractor::ActiveOrg {
    use crate::orgs::extractor::{resolve_active_org, unpack, ActiveOrg, ACTIVE_ORG_COOKIE};
    use crate::queries::orgs::{ensure_personal_org, list_memberships};

    let personal_org_id = match ensure_personal_org(pool, user_id).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("ensure_personal_org failed for user {user_id}: {e:#}");
            return ActiveOrg { org_id: 1, role: Some(crate::orgs::Role::Member) };
        }
    };

    let memberships = match list_memberships(pool, user_id).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("list_memberships failed for user {user_id}: {e:#}");
            return ActiveOrg { org_id: personal_org_id, role: Some(crate::orgs::Role::Member) };
        }
    };

    let cookie_org = encryptor.and_then(|enc| {
        crate::middleware::cookie::read_cookie(headers, ACTIVE_ORG_COOKIE)
            .and_then(|v| unpack(enc, v))
    });

    let member_ids: Vec<i64> = memberships.iter().map(|m| m.org_id).collect();
    let org_id = resolve_active_org(cookie_org, &member_ids, personal_org_id);

    let role = memberships
        .iter()
        .find(|m| m.org_id == org_id)
        .map(|m| crate::orgs::Role::parse(&m.role))
        .unwrap_or(crate::orgs::Role::Member);

    ActiveOrg { org_id, role: Some(role) }
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

/// True when the session has exceeded its absolute lifetime cap. `cap_secs == 0` disables the check.
fn session_expired(created_at: i64, now: i64, cap_secs: u64) -> bool {
    cap_secs > 0 && now - created_at > cap_secs as i64
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

#[cfg(test)]
mod tests {
    use super::{is_public_path, session_expired};

    #[test]
    fn session_not_expired_strictly_under_cap() {
        // elapsed < cap: not yet expired
        assert!(!session_expired(1000, 1000 + 3599, 3600));
    }

    #[test]
    fn session_expired_just_over_cap() {
        // elapsed > cap: expired
        assert!(session_expired(1000, 1000 + 3601, 3600));
    }

    #[test]
    fn session_at_boundary_not_expired() {
        // elapsed == cap: exactly at boundary, not yet expired
        assert!(!session_expired(0, 3600, 3600));
    }

    #[test]
    fn cap_zero_disables_expiry() {
        // cap == 0 disables the check
        assert!(!session_expired(0, i64::MAX, 0));
    }

    #[test]
    fn only_pre_session_oauth_paths_are_public() {
        for p in [
            "/web/login",
            "/health",
            "/web/_assets/style.css",
            "/web/auth/login",
            "/web/auth/callback",
            "/web/auth/backchannel-logout",
        ] {
            assert!(is_public_path(p), "{p} must be public");
        }
        // Gated: logout still runs the gate to resolve + tear down the caller's
        // own session. A blanket `/web/auth/` match here once broke OIDC logout.
        for p in [
            "/web/logout",
            "/web/auth/logout",
            "/web/projects/",
            "/web/settings/integrations/",
        ] {
            assert!(!is_public_path(p), "{p} must be gated");
        }
    }
}
