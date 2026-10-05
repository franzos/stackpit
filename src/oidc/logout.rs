//! Logout primitives.
//!
//! Two paths land here:
//! - **Local + RP-initiated**: the user clicks "Log out". We delete the
//!   grant row, clear the cookie, and bounce to the IdP's
//!   `end_session_endpoint` (OIDC RP-Initiated Logout 1.0 §2.1) so the IdP
//!   session also goes away. When the IdP doesn't advertise the endpoint,
//!   the flow degrades to a local-only logout.
//! - **Back-channel**: the IdP POSTs a signed `logout_token` JWT to
//!   `/web/auth/backchannel-logout` (OIDC Back-Channel Logout 1.0 §2.4). We
//!   validate it strictly (no nonce, `events` claim present, `sid` or
//!   `sub`, etc.), insert a revocation marker, and eager-delete matching
//!   grant rows.

use oidc_relying_party::logout_token::{self, LogoutTokenClaims, LogoutTokenPolicy};

use crate::oidc::client::OidcClient;
use crate::oidc::revocations;

/// Validate a back-channel logout token (OIDC Back-Channel Logout 1.0 §2.6)
/// against the provider's keys. `None` means 400; the spec forbids echoing
/// details, so the reason goes to the log only.
///
/// JTI replay defense lives one layer up in the handler (a DB write, not a
/// token-validation concern).
pub async fn validate_logout_token(oidc: &OidcClient, token: &str) -> Option<LogoutTokenClaims> {
    let policy = LogoutTokenPolicy::new(oidc.issuer(), oidc.client_id());
    match logout_token::validate_logout_token(
        oidc.jwks_cache(),
        &oidc.discovery().signing_algorithms,
        &policy,
        token,
    )
    .await
    {
        Ok(claims) => Some(claims),
        Err(e) => {
            tracing::warn!(error = %e, "back-channel logout token rejected");
            None
        }
    }
}

/// TTL for revocation markers and JTI dedupe. Sized to the larger of the
/// access- or refresh-token ceilings so the marker outlives any replayable
/// pre-logout token. Providers vary on exposing `refresh_token_exp`; the
/// ceiling is pinned via `refresh_token_max_ttl_secs` when it's missing.
/// Floored at 60s so tiny ceilings still produce a usable marker.
pub fn revocation_ttl_secs(access_token_max_ttl_secs: u64, refresh_token_max_ttl_secs: u64) -> i64 {
    access_token_max_ttl_secs
        .max(refresh_token_max_ttl_secs)
        .max(60) as i64
}

/// Given a [`LogoutValidation::Ok`], end the sessions it names: by `sid`
/// when present (that session only, even if `sub` is there too, §2.6),
/// else every session of `sub`. A `jti` seen before is a no-op replay. The
/// `jti` is recorded only after the logout landed, so a failed apply leaves
/// the IdP's retry free to succeed.
pub async fn apply_logout(
    pool: &crate::db::DbPool,
    iss: &str,
    sub: Option<&str>,
    sid: Option<&str>,
    jti: &str,
    iat: i64,
    revocation_ttl_secs: i64,
) -> Result<(), LogoutApplyError> {
    let expires_at = iat + revocation_ttl_secs;

    match revocations::jti_seen(pool, jti).await {
        Ok(true) => return Err(LogoutApplyError::Replay),
        Ok(false) => {}
        Err(e) => return Err(LogoutApplyError::Db(e)),
    }

    if let Some(sid) = sid.filter(|s| !s.is_empty()) {
        revocations::insert_sid(pool, iss, sid, expires_at)
            .await
            .map_err(LogoutApplyError::Db)?;
        crate::oidc::grants::delete_by_sid(pool, iss, sid)
            .await
            .map_err(LogoutApplyError::Db)?;
    } else if let Some(sub) = sub.filter(|s| !s.is_empty()) {
        revocations::insert_sub(pool, iss, sub, expires_at)
            .await
            .map_err(LogoutApplyError::Db)?;
        crate::oidc::grants::delete_by_sub(pool, iss, sub)
            .await
            .map_err(LogoutApplyError::Db)?;
    }
    // A concurrent duplicate may have recorded it first; both applied the
    // same idempotent logout, so that's not an error.
    revocations::jti_seen_or_remember(pool, jti, expires_at)
        .await
        .map_err(LogoutApplyError::Db)?;
    Ok(())
}

#[derive(Debug)]
pub enum LogoutApplyError {
    /// JTI seen before; already applied, so the handler answers 200.
    Replay,
    Db(anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_picks_the_larger_of_access_and_refresh() {
        // refresh outlasts access (the common case).
        assert_eq!(revocation_ttl_secs(3600, 14 * 24 * 3600), 14 * 24 * 3600);
        // access outlasts refresh (e.g. operator set refresh=0).
        assert_eq!(revocation_ttl_secs(7200, 60), 7200);
        // equal values pass through.
        assert_eq!(revocation_ttl_secs(3600, 3600), 3600);
    }

    #[test]
    fn ttl_floors_at_sixty_seconds() {
        assert_eq!(revocation_ttl_secs(0, 0), 60);
        assert_eq!(revocation_ttl_secs(10, 30), 60);
        assert_eq!(revocation_ttl_secs(59, 59), 60);
        // One second above the floor: no clamp.
        assert_eq!(revocation_ttl_secs(61, 0), 61);
    }

    use oidc_relying_party::jwks::JwksCacheConfig;
    use oidc_relying_party::test_support::{jwks, logout_token_claims, rsa_key, sign_logout_token};

    const ISS: &str = "https://hydra.example.com";
    const AUD: &str = "stackpit-web";

    #[cfg(feature = "sqlite")]
    fn valid_now() -> i64 {
        chrono::Utc::now().timestamp()
    }

    fn primed_oidc() -> OidcClient {
        let cache = stackpit_auth::JwksCache::new(
            reqwest::Client::new(),
            url::Url::parse("http://127.0.0.1:0/jwks").unwrap(),
            JwksCacheConfig::default(),
        );
        cache
            .prime_raw(&jwks(&[rsa_key()]).to_string())
            .expect("prime");
        OidcClient::for_test(ISS.to_string(), AUD.to_string(), cache)
    }

    fn signed_logout_token(sub: Option<&str>, sid: Option<&str>) -> String {
        let key = rsa_key();
        let claims = logout_token_claims(ISS, AUD, sub, sid);
        sign_logout_token(&key, Some(key.kid()), Some("logout+jwt"), &claims)
    }

    #[tokio::test]
    async fn signed_valid_logout_token_ok() {
        let claims =
            validate_logout_token(&primed_oidc(), &signed_logout_token(Some("alice"), None))
                .await
                .expect("valid logout token");
        assert_eq!(claims.sub.as_deref(), Some("alice"));
        assert_eq!(claims.iss, ISS);
    }

    // --- jti replay (in-memory SQLite via crate::db::open_test_pool) ---

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn apply_logout_second_jti_is_replay() {
        let pool = crate::db::open_test_pool().await;
        let iat = valid_now();
        let ttl = revocation_ttl_secs(3600, 3600);

        let first = apply_logout(&pool, ISS, Some("alice"), None, "dup-jti", iat, ttl).await;
        assert!(first.is_ok(), "first apply should succeed");

        let second = apply_logout(&pool, ISS, Some("alice"), None, "dup-jti", iat, ttl).await;
        assert!(
            matches!(second, Err(LogoutApplyError::Replay)),
            "second apply with same jti must be a replay"
        );
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn replayed_logout_token_returns_ok() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let pool = crate::db::open_test_pool().await;
        let (mut state, _chans) = crate::server::AppState::for_test(pool);
        state.oidc = crate::oidc::discovery::OidcSlot::ready(crate::oidc::discovery::OidcReady {
            client: std::sync::Arc::new(primed_oidc()),
        });
        let app = axum::Router::new()
            .route(
                "/bc",
                axum::routing::post(crate::html::auth::backchannel_logout),
            )
            .with_state(state);
        let token = signed_logout_token(Some("alice"), Some("sid-1"));
        for _ in 0..2 {
            let res = app
                .clone()
                .oneshot(
                    Request::post("/bc")
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from(format!("logout_token={token}")))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
        }
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[tokio::test]
    async fn logout_token_with_sid_and_sub_ends_only_that_session() {
        use crate::oidc::grants::{self, NewGrant};
        let pool = crate::queries::test_helpers::open_test_db().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, ISS, "alice", None, None)
            .await
            .unwrap();
        let enc = crate::util::crypto::SecretEncryptor::from_config_or_env(Some(
            &secrecy::SecretString::from("11".repeat(32)),
        ))
        .unwrap()
        .unwrap();
        let mut handles = Vec::new();
        for sid in ["sid-1", "sid-2"] {
            handles.push(
                grants::insert(
                    &pool,
                    &enc,
                    &NewGrant {
                        user_id: u.user_id,
                        iss: ISS,
                        sub: "alice",
                        sid: Some(sid),
                        access_token: "at",
                        access_exp: valid_now() + 3600,
                        refresh_token: None,
                        refresh_exp: None,
                        id_token: "it",
                        offline: false,
                    },
                )
                .await
                .unwrap(),
            );
        }
        apply_logout(
            &pool,
            ISS,
            Some("alice"),
            Some("sid-1"),
            "jti-sid",
            valid_now(),
            3600,
        )
        .await
        .unwrap();
        assert!(grants::load(&pool, &enc, &handles[0])
            .await
            .unwrap()
            .is_none());
        assert!(grants::load(&pool, &enc, &handles[1])
            .await
            .unwrap()
            .is_some());
    }
}
