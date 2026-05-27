//! Logout primitives.
//!
//! Two paths land here:
//! - **Local + RP-initiated**: the user clicks "Log out". We delete the
//!   grant row, clear the cookie, and bounce to the IdP's
//!   `end_session_endpoint` (OIDC RP-Initiated Logout 1.0 §2.1) so the IdP
//!   session also goes away. Hydra advertises this endpoint; some IdPs do
//!   not -- the flow degrades to a local-only logout when it's absent.
//! - **Back-channel**: the IdP POSTs a signed `logout_token` JWT to
//!   `/web/auth/backchannel-logout` (OIDC Back-Channel Logout 1.0 §2.4). We
//!   validate it strictly (no nonce, `events` claim present, `sid` or
//!   `sub`, etc.), insert a revocation marker, and eager-delete matching
//!   grant rows.
//!
//! The integration guide drove every claim check here; see
//! `~/git/forseti/docs/integration-guide.md#logout`.

use anyhow::{Context, Result};
use jsonwebtoken::{Algorithm, Validation};
use serde::Deserialize;

use crate::oauth::OidcClient;
use crate::oidc::revocations;

/// Conservative window for `iat`: the spec doesn't pin a value but Stripe-
/// shape integrations and the Ory examples use 60s. Logout tokens are
/// emitted fresh per delivery, so anything older is almost certainly stale.
const LOGOUT_IAT_TOLERANCE_SECS: i64 = 60;

/// Claims we accept on a back-channel logout token. Only the fields we
/// actually validate -- extra claims are ignored.
#[derive(Debug, Deserialize)]
pub struct LogoutTokenClaims {
    pub iss: String,
    /// Spec allows string or array; we accept either and normalise downstream.
    #[serde(default)]
    pub aud: Aud,
    pub iat: i64,
    pub jti: String,
    /// MUST be present; MUST contain the back-channel logout event marker
    /// pointing at a JSON object (typically `{}`).
    pub events: serde_json::Map<String, serde_json::Value>,
    pub sub: Option<String>,
    pub sid: Option<String>,
    /// Spec FORBIDS this. Presence is a hard reject.
    #[serde(default)]
    pub nonce: Option<serde_json::Value>,
    /// Spec FORBIDS this on logout tokens (only id_tokens carry exp).
    /// We deserialize as Value so any present claim -- even null -- trips
    /// the rejection. jsonwebtoken's `validate_exp = false` would otherwise
    /// silently accept it.
    #[serde(default)]
    pub exp: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(untagged)]
pub enum Aud {
    #[default]
    Missing,
    One(String),
    Many(Vec<String>),
}

impl Aud {
    fn contains(&self, expected: &str) -> bool {
        match self {
            Aud::Missing => false,
            Aud::One(s) => s == expected,
            Aud::Many(v) => v.iter().any(|a| a == expected),
        }
    }
}

/// Outcome of validating a logout token. `Invalid` means 400; the spec is
/// emphatic that we MUST NOT echo any details (no body), so the variant
/// carries no message field -- diagnostics go to logs only.
pub enum LogoutValidation {
    /// Signature + claims passed. Caller writes revocations + deletes grants.
    Ok {
        iss: String,
        sub: Option<String>,
        sid: Option<String>,
        jti: String,
        iat: i64,
    },
    Invalid,
}

/// Validate a back-channel logout token. Steps:
/// 1. Parse JWT header → resolve key from JWKS by `kid`.
/// 2. Verify RS256 signature.
/// 3. Validate claims per [OpenID Connect Back-Channel Logout 1.0](https://openid.net/specs/openid-connect-backchannel-1_0.html#LogoutToken):
///    `iss`, `aud`, `iat` freshness, `events` event marker, no `nonce`,
///    at least one of `sub` / `sid` present.
///
/// Note: JTI replay defense lives one layer up in the handler -- it's a DB
/// write, not a token-validation concern.
pub async fn validate_logout_token(oidc: &OidcClient, token: &str) -> LogoutValidation {
    match validate_logout_token_inner(oidc, token).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "back-channel logout token rejected");
            LogoutValidation::Invalid
        }
    }
}

async fn validate_logout_token_inner(oidc: &OidcClient, token: &str) -> Result<LogoutValidation> {
    // Crypto + signed-claim verification (kid -> JWKS -> RS256 + iss/aud) is
    // the shared primitive. The validator below carries the logout-specific
    // policy: exp is disabled (logout tokens carry no exp -- the iat freshness
    // window is the freshness gate), and the spec checks that jsonwebtoken
    // can't express run afterwards.
    let mut v = Validation::new(Algorithm::RS256);
    v.set_issuer(&[oidc.issuer()]);
    v.set_audience(&[oidc.client_id()]);
    v.validate_exp = false;
    v.set_required_spec_claims(&["iss", "aud", "iat"]);

    let claims: LogoutTokenClaims = oidc
        .jwks_cache()
        .verify_rs256(token, &v)
        .await
        .context("logout token signature/claim validation failed")?;

    // Step 4: spec-specific checks that jsonwebtoken doesn't cover.
    // 4a) Audience must include our client_id (jsonwebtoken's check is
    //     defensive but it expects `aud` to deserialize as Vec<String>; we
    //     accept string-or-array manually for safety).
    if !claims.aud.contains(oidc.client_id()) {
        anyhow::bail!("audience does not include client_id");
    }
    // 4b) `nonce` MUST NOT be present.
    if claims.nonce.is_some() {
        anyhow::bail!("logout token carries forbidden `nonce` claim");
    }
    // 4c) `exp` MUST NOT be present on logout tokens (spec §2.4).
    //     jsonwebtoken has validate_exp disabled, so we check presence ourselves.
    if claims.exp.is_some() {
        anyhow::bail!("logout token carries forbidden `exp` claim");
    }
    // 4d) `events` must contain the back-channel logout marker pointing at
    //     a JSON object (typically `{}`). A scalar or null value is a
    //     spec violation and a sign the IdP is mis-emitting.
    const LOGOUT_EVENT_MARKER: &str = "http://schemas.openid.net/event/backchannel-logout";
    match claims.events.get(LOGOUT_EVENT_MARKER) {
        Some(v) if v.is_object() => { /* spec-compliant */ }
        Some(_) => anyhow::bail!("`events` marker value is not a JSON object"),
        None => anyhow::bail!("`events` claim missing back-channel logout marker"),
    }
    // 4e) At least one of sub / sid required.
    if claims.sub.is_none() && claims.sid.is_none() {
        anyhow::bail!("logout token missing both sub and sid");
    }
    // 4f) Freshness: iat must be within tolerance window. Reject "now" off
    //     by more than LOGOUT_IAT_TOLERANCE_SECS on either side.
    let now = chrono::Utc::now().timestamp();
    if (now - claims.iat).abs() > LOGOUT_IAT_TOLERANCE_SECS {
        anyhow::bail!("logout token iat outside tolerance window");
    }

    Ok(LogoutValidation::Ok {
        iss: claims.iss,
        sub: claims.sub,
        sid: claims.sid,
        jti: claims.jti,
        iat: claims.iat,
    })
}

/// Build the RP-initiated logout URL (OIDC RP-Initiated Logout 1.0 §2.1):
/// redirect the user to the IdP's `end_session_endpoint` with
/// `id_token_hint` + `post_logout_redirect_uri` so the IdP knows whose
/// session to destroy and where to send them after.
///
/// `post_logout_redirect_uri` here is *operator config* read from
/// `auth.oauth.post_logout_redirect_uri`, validated at startup
/// (`Config::validate` rejects non-http(s) values and cross-origin targets
/// unless explicitly allowed). No HTTP request data reaches this argument.
/// If a future change ever pipes user input (a header, query string, or
/// cookie content) into this slot, revalidate per request -- Hydra's
/// `end_session_endpoint` will happily echo whatever it gets to the
/// browser.
pub fn build_end_session_url(
    end_session_endpoint: &str,
    id_token_hint: &str,
    post_logout_redirect_uri: Option<&str>,
) -> String {
    let mut out = String::with_capacity(end_session_endpoint.len() + 256);
    out.push_str(end_session_endpoint);
    out.push(if end_session_endpoint.contains('?') {
        '&'
    } else {
        '?'
    });
    out.push_str("id_token_hint=");
    out.push_str(&urlencode(id_token_hint));
    if let Some(post) = post_logout_redirect_uri {
        out.push_str("&post_logout_redirect_uri=");
        out.push_str(&urlencode(post));
    }
    out
}

fn urlencode(s: &str) -> String {
    // form_urlencoded is already in deps for OAuth params; use the same
    // percent-encoding rules (application/x-www-form-urlencoded space-as-+).
    form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Helper for the handler: compute the TTL for revocation markers + JTI
/// dedupe. Sized to whichever of the access- or refresh-token ceilings is
/// larger so the marker still fires while either kind of pre-logout token
/// could still be replayed. OIDC providers vary on whether the token
/// response includes `refresh_token_exp`; we treat the field as "unknown
/// lifetime" and bound it via `refresh_token_max_ttl_secs` (Hydra is the
/// common case where it's missing, so the operator pins the ceiling via
/// config).
///
/// Floored at 60 seconds so absurdly low ceilings still produce a usable
/// marker.
pub fn revocation_ttl_secs(access_token_max_ttl_secs: u64, refresh_token_max_ttl_secs: u64) -> i64 {
    access_token_max_ttl_secs
        .max(refresh_token_max_ttl_secs)
        .max(60) as i64
}

/// Pure helper: given a [`LogoutValidation::Ok`], write the revocation
/// marker (sid preferred, sub fallback) and JTI dedupe row.
///
/// Returns `Err(())` if the JTI was already seen (replay) -- caller MUST
/// return 400 in that case.
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

    // JTI replay defense first -- if it's a replay, do nothing else.
    match revocations::jti_seen_or_remember(pool, jti, expires_at).await {
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
    Ok(())
}

#[derive(Debug)]
pub enum LogoutApplyError {
    /// JTI seen before; back-channel handler returns 400.
    Replay,
    Db(anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_picks_the_larger_of_access_and_refresh() {
        // refresh outlasts access -- the common case (e.g. Hydra defaults).
        assert_eq!(revocation_ttl_secs(3600, 14 * 24 * 3600), 14 * 24 * 3600);
        // access outlasts refresh -- e.g. operator set refresh=0 to disable.
        assert_eq!(revocation_ttl_secs(7200, 60), 7200);
        // equal values -- either side wins, both pass through.
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
}
