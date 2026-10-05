//! Refresh-token rotation: pull a fresh access token from the IdP and write
//! it back to the grant row.
//!
//! Refresh-token rotation behaviour is provider-dependent. OAuth 2.1 §4.3.2
//! recommends rotating refresh tokens on every use (the response carries a
//! new refresh_token, the old one is invalidated); Hydra does this by
//! default. Concurrent refresh against the same grant: the IdP accepts the
//! first request and returns `invalid_grant` to the second. The loser
//! surfaces as [`RefreshOutcome::InvalidGrant`] and the middleware bounces
//! the user to login. Acceptable; rare.
//!
//! **Race shape (read-check-act, intentional):** the loser of the
//! token-endpoint race re-reads the grant row on `invalid_grant` and rides
//! the winner's rotated tokens when `access_token` has changed beneath it.
//! That's a deliberate optimistic-concurrency pattern: two refreshes against
//! the same grant in flight at the same time are rare in practice (browser
//! tabs rarely fire refreshes within milliseconds of each other), and the
//! penalty for the loser is one extra DB read instead of a forced re-login.
//! Per-handle async mutexes would close the window completely but would also
//! serialise every refresh through a hot lock -- not worth it until the race
//! rate stops being negligible.
//!
//! **Escalation threshold:** if `stackpit_oidc_refresh_race_won` rises above
//! ~1% of `stackpit_oidc_refresh_attempts` over a 24h window, escalate to a
//! per-handle async mutex (keyed by `grant.handle`) around the
//! `oidc.refresh()` call. Until then the instrumentation below stays as
//! observability only.

use anyhow::Result;

use crate::db::DbPool;
use crate::oidc::client::{OidcClient, RefreshError};
use crate::oidc::grants::{self, GrantRecord};
use crate::util::crypto::SecretEncryptor;

#[allow(clippy::large_enum_variant)]
pub enum RefreshOutcome {
    /// New tokens persisted; caller should use the updated record.
    Refreshed(GrantRecord),
    /// IdP rejected the refresh token (`invalid_grant`). Caller must force re-login.
    InvalidGrant,
    /// Network / transient failure. Caller can retry with the existing
    /// access token (still valid up to its `access_exp`).
    Transient(String),
}

/// Run a refresh-token exchange and persist the new tokens. Returns the
/// updated [`GrantRecord`] on success.
pub async fn refresh(
    pool: &DbPool,
    encryptor: &SecretEncryptor,
    oidc: &OidcClient,
    grant: &GrantRecord,
) -> Result<RefreshOutcome> {
    // Observability: every refresh attempt is logged on the `stackpit::oidc`
    // target so log-grepping tooling can derive a counter. Renamed to a real
    // metrics counter once the codebase grows a metrics crate.
    tracing::info!(
        target: "stackpit::oidc",
        metric = "stackpit_oidc_refresh_attempts",
        "oidc refresh attempted",
    );

    let Some(refresh_token) = grant.refresh_token.as_deref() else {
        // No refresh token: nothing to do. Caller treats this as "access
        // token is the only thing we have; let it run until expiry".
        return Ok(RefreshOutcome::Transient(
            "no refresh token on this grant".into(),
        ));
    };

    match oidc.refresh(refresh_token, &grant.sub).await {
        Ok(new_tokens) => {
            // Persist immediately; race losers get InvalidGrant on next call.
            grants::rotate_tokens(
                pool,
                encryptor,
                &grant.handle,
                &new_tokens,
                new_tokens.id_token.as_deref(),
                new_tokens.offline,
            )
            .await?;

            let updated = GrantRecord {
                handle: grant.handle.clone(),
                user_id: grant.user_id,
                iss: grant.iss.clone(),
                sub: grant.sub.clone(),
                sid: grant.sid.clone(),
                access_token: new_tokens.access_token,
                access_exp: new_tokens.access_exp,
                refresh_token: new_tokens
                    .refresh_token
                    .or_else(|| grant.refresh_token.clone()),
                refresh_exp: new_tokens.refresh_exp.or(grant.refresh_exp),
                id_token: new_tokens.id_token.or_else(|| grant.id_token.clone()),
                csrf_token: grant.csrf_token.clone(),
                created_at: grant.created_at,
                offline: new_tokens.offline.unwrap_or(grant.offline),
            };
            Ok(RefreshOutcome::Refreshed(updated))
        }
        Err(RefreshError::InvalidGrant) => {
            // Concurrent-refresh race: another request beat us to the token
            // endpoint, the IdP invalidated our refresh_token, and the row
            // now holds the winner's freshly-rotated tokens. Re-read the row;
            // if the access_token changed beneath us, the user is still
            // logged in -- ride the rotation. Only force re-login when the
            // row actually hasn't been rotated (i.e. the IdP really did
            // revoke the grant, not just rotate it).
            match grants::load(pool, encryptor, &grant.handle).await {
                Ok(Some(reloaded)) if reloaded.access_token != grant.access_token => {
                    tracing::info!(
                        target: "stackpit::oidc",
                        metric = "stackpit_oidc_refresh_race_won",
                        race = "won",
                        "concurrent refresh detected; rode rotation",
                    );
                    Ok(RefreshOutcome::Refreshed(reloaded))
                }
                _ => {
                    tracing::info!(
                        target: "stackpit::oidc",
                        metric = "stackpit_oidc_refresh_race_lost",
                        race = "lost",
                        "invalid_grant with no visible rotation; forcing re-login",
                    );
                    Ok(RefreshOutcome::InvalidGrant)
                }
            }
        }
        Err(RefreshError::Transient(msg)) => Ok(RefreshOutcome::Transient(msg)),
    }
}

#[cfg(all(test, feature = "sqlite", not(feature = "postgres")))]
mod tests {
    use super::*;
    use crate::oidc::grants::NewGrant;
    use oidc_relying_party::algorithms::SigningAlgorithm;
    use oidc_relying_party::test_support::{
        jwks, rsa_key, sign_id_token, unix_now, FakeIdp, TOKEN_PATH,
    };
    use serde_json::json;

    const CLIENT_ID: &str = "stackpit-web";

    /// A grant for `alice` refreshed against a fake IdP whose token endpoint
    /// answers with `status` and `body(issuer)`.
    async fn refresh_against(status: usize, body: impl FnOnce(&str) -> String) -> RefreshOutcome {
        let key = rsa_key();
        let mut idp = FakeIdp::start(&[SigningAlgorithm::Rs256], &jwks(&[key]), json!({})).await;
        let issuer = idp.issuer();
        idp.server()
            .mock("POST", TOKEN_PATH)
            .with_status(status)
            .with_header("content-type", "application/json")
            .with_body(body(&issuer))
            .create_async()
            .await;
        let oidc = OidcClient::for_test(
            issuer.clone(),
            CLIENT_ID.to_string(),
            stackpit_auth::JwksCache::new(
                reqwest::Client::new(),
                url::Url::parse(&idp.jwks_url()).unwrap(),
                crate::oidc::client::jwks_cache_config(60),
            ),
        );

        let pool = crate::queries::test_helpers::open_test_db().await;
        let user = crate::queries::users::upsert_from_oidc(&pool, &issuer, "alice", None, None)
            .await
            .unwrap();
        let enc = SecretEncryptor::from_config_or_env(Some(&secrecy::SecretString::from(
            "11".repeat(32),
        )))
        .unwrap()
        .unwrap();
        let handle = grants::insert(
            &pool,
            &enc,
            &NewGrant {
                user_id: user.user_id,
                iss: &issuer,
                sub: "alice",
                sid: None,
                access_token: "at1",
                access_exp: 0,
                refresh_token: Some("rt1"),
                refresh_exp: None,
                id_token: "old-id-token",
                offline: false,
            },
        )
        .await
        .unwrap();
        let grant = grants::load(&pool, &enc, &handle).await.unwrap().unwrap();
        refresh(&pool, &enc, &oidc, &grant).await.unwrap()
    }

    fn token_response(issuer: &str, sub: &str) -> String {
        let now = unix_now();
        let id_token = sign_id_token(
            &rsa_key(),
            Some(rsa_key().kid()),
            &json!({
                "iss": issuer,
                "sub": sub,
                "aud": CLIENT_ID,
                "iat": now,
                "exp": now + 300,
            }),
        );
        json!({
            "access_token": "at2",
            "token_type": "Bearer",
            "expires_in": 300,
            "id_token": id_token,
        })
        .to_string()
    }

    #[tokio::test]
    async fn refreshed_id_token_for_the_same_sub_is_kept() {
        let outcome = refresh_against(200, |iss| token_response(iss, "alice")).await;
        let RefreshOutcome::Refreshed(grant) = outcome else {
            panic!("expected Refreshed");
        };
        assert_eq!(grant.access_token, "at2");
        assert_ne!(grant.id_token.as_deref(), Some("old-id-token"));
    }

    #[tokio::test]
    async fn refreshed_id_token_for_another_sub_ends_the_grant_like_invalid_grant() {
        let invalid_grant =
            refresh_against(400, |_| json!({ "error": "invalid_grant" }).to_string()).await;
        assert!(matches!(invalid_grant, RefreshOutcome::InvalidGrant));

        let other_sub = refresh_against(200, |iss| token_response(iss, "mallory")).await;
        assert!(matches!(other_sub, RefreshOutcome::InvalidGrant));
    }
}
