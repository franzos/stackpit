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

use crate::crypto::SecretEncryptor;
use crate::db::DbPool;
use crate::oauth::{OidcClient, RefreshError};
use crate::oidc::grants::{self, GrantRecord};

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

    match oidc.refresh(refresh_token).await {
        Ok(new_tokens) => {
            // Persist immediately; race losers get InvalidGrant on next call.
            grants::rotate_tokens(
                pool,
                encryptor,
                &grant.handle,
                &new_tokens.access_token,
                new_tokens.access_exp,
                new_tokens.refresh_token.as_deref(),
                new_tokens.refresh_exp,
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
                id_token: grant.id_token.clone(),
                csrf_token: grant.csrf_token.clone(),
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
