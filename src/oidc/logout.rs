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

use anyhow::{Context, Result};
use jsonwebtoken::{Algorithm, Validation};
use serde::Deserialize;

use crate::oauth::OidcClient;
use crate::oidc::revocations;

/// Conservative `iat` window: the spec doesn't pin a value, but common
/// integrations use 60s. Logout tokens are emitted fresh per delivery, so
/// anything older is almost certainly stale.
const LOGOUT_IAT_TOLERANCE_SECS: i64 = 60;

/// Claims we accept on a back-channel logout token. Only the validated
/// fields; extra claims are ignored.
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
    /// Spec FORBIDS this; presence is a hard reject. Double-option keeps an
    /// absent key (`None`) distinct from present-but-null (`Some(None)`);
    /// both forms of presence trip the rejection.
    #[serde(default, deserialize_with = "double_option")]
    pub nonce: Option<Option<serde_json::Value>>,
    /// Spec FORBIDS this on logout tokens (only id_tokens carry exp).
    /// `validate_exp = false` would otherwise silently accept it. Double-option
    /// so any present `exp` claim (even null) trips the rejection, while a
    /// truly absent claim is fine.
    #[serde(default, deserialize_with = "double_option")]
    pub exp: Option<Option<serde_json::Value>>,
}

/// Deserialize into a double option: an absent key yields `None` (via
/// `#[serde(default)]`) while a present key, including explicit JSON `null`,
/// yields `Some(_)`. Lets presence checks reject present-null claims that a
/// plain `Option` would collapse to `None`.
fn double_option<'de, D, T>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(de).map(Some)
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

/// Outcome of validating a logout token. `Invalid` means 400; the spec
/// requires we MUST NOT echo details (no body), so the variant carries no
/// message field (diagnostics go to logs only).
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
/// Note: JTI replay defense lives one layer up in the handler (a DB write,
/// not a token-validation concern).
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
    // Shared primitive: kid -> JWKS -> RS256 + iss/aud. The validator below
    // carries logout-specific policy: exp is disabled (logout tokens carry no
    // exp; iat freshness is the gate). Spec checks jsonwebtoken can't express
    // run afterwards.
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

    check_logout_claims(
        &claims,
        oidc.issuer(),
        oidc.client_id(),
        chrono::Utc::now().timestamp(),
    )?;

    Ok(LogoutValidation::Ok {
        iss: claims.iss,
        sub: claims.sub,
        sid: claims.sid,
        jti: claims.jti,
        iat: claims.iat,
    })
}

/// Back-channel logout claim policy (OIDC Back-Channel Logout 1.0 §2.4),
/// independent of the signature/JWKS layer so it's unit-testable. Re-checks
/// iss/aud (aud parsed manually as string-or-array) so the helper is
/// self-contained even though `Validation` also enforces them.
fn check_logout_claims(
    claims: &LogoutTokenClaims,
    expected_iss: &str,
    expected_aud: &str,
    now_secs: i64,
) -> Result<()> {
    const LOGOUT_EVENT_MARKER: &str = "http://schemas.openid.net/event/backchannel-logout";

    if claims.iss != expected_iss {
        anyhow::bail!("issuer does not match expected");
    }
    // Audience must include our client_id (jsonwebtoken's check is defensive
    // but it expects `aud` to deserialize as Vec<String>; we accept
    // string-or-array manually for safety).
    if !claims.aud.contains(expected_aud) {
        anyhow::bail!("audience does not include client_id");
    }
    // `nonce` MUST NOT be present (any present key, even null, rejects).
    if claims.nonce.is_some() {
        anyhow::bail!("logout token carries forbidden `nonce` claim");
    }
    // `exp` MUST NOT be present on logout tokens (spec §2.4). jsonwebtoken has
    // validate_exp disabled, so we check presence ourselves (any present key,
    // even null, rejects).
    if claims.exp.is_some() {
        anyhow::bail!("logout token carries forbidden `exp` claim");
    }
    // `events` must contain the back-channel logout marker pointing at a JSON
    // object (typically `{}`). A scalar or null value is a spec violation and
    // a sign the IdP is mis-emitting.
    match claims.events.get(LOGOUT_EVENT_MARKER) {
        Some(v) if v.is_object() => {}
        Some(_) => anyhow::bail!("`events` marker value is not a JSON object"),
        None => anyhow::bail!("`events` claim missing back-channel logout marker"),
    }
    // At least one of sub / sid required.
    if claims.sub.is_none() && claims.sid.is_none() {
        anyhow::bail!("logout token missing both sub and sid");
    }
    // Freshness: iat must be within tolerance on either side.
    if (now_secs - claims.iat).abs() > LOGOUT_IAT_TOLERANCE_SECS {
        anyhow::bail!("logout token iat outside tolerance window");
    }

    Ok(())
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
/// cookie content) into this slot, revalidate per request: the IdP's
/// `end_session_endpoint` will echo whatever it gets to the browser.
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
    form_urlencoded::byte_serialize(s.as_bytes()).collect()
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

/// Given a [`LogoutValidation::Ok`], write the revocation marker (sid
/// preferred, sub fallback) and JTI dedupe row. Returns a replay error if
/// the JTI was already seen; caller MUST return 400 in that case.
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

    // JTI replay defense first: on replay, do nothing else.
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

    // --- Claim-policy unit tests (no signing; serde drives defaults) ---

    use serde_json::{json, Value};

    const ISS: &str = "https://hydra.example.com";
    const AUD: &str = "stackpit-web";
    const MARKER: &str = "http://schemas.openid.net/event/backchannel-logout";

    /// Build claims through serde so `#[serde(default)]` on nonce/exp and
    /// `Aud::Missing` behave exactly as in production.
    fn claims_from(base: Value) -> LogoutTokenClaims {
        serde_json::from_value(base).expect("test claims deserialize")
    }

    fn valid_now() -> i64 {
        chrono::Utc::now().timestamp()
    }

    fn check(c: &LogoutTokenClaims, now: i64) -> Result<()> {
        check_logout_claims(c, ISS, AUD, now)
    }

    #[test]
    fn claims_valid_sub_only_ok() {
        let now = valid_now();
        let c = claims_from(json!({
            "iss": ISS, "aud": AUD, "iat": now, "jti": "j1",
            "events": { MARKER: {} }, "sub": "alice",
        }));
        assert!(check(&c, now).is_ok());
    }

    #[test]
    fn claims_valid_sid_only_ok() {
        let now = valid_now();
        let c = claims_from(json!({
            "iss": ISS, "aud": AUD, "iat": now, "jti": "j1",
            "events": { MARKER: {} }, "sid": "session-7",
        }));
        assert!(check(&c, now).is_ok());
    }

    #[test]
    fn claims_valid_both_sub_and_sid_ok() {
        let now = valid_now();
        let c = claims_from(json!({
            "iss": ISS, "aud": AUD, "iat": now, "jti": "j1",
            "events": { MARKER: {} }, "sub": "alice", "sid": "session-7",
        }));
        assert!(check(&c, now).is_ok());
    }

    #[test]
    fn claims_nonce_present_rejected() {
        let now = valid_now();
        let c = claims_from(json!({
            "iss": ISS, "aud": AUD, "iat": now, "jti": "j1",
            "events": { MARKER: {} }, "sub": "alice", "nonce": "abc",
        }));
        assert!(check(&c, now).is_err(), "nonce present (value) must reject");
    }

    // A present `nonce` key, even with a literal JSON `null` value, is a spec
    // violation (§2.4: nonce MUST NOT be present). The double-option keeps
    // present-null (`Some(None)`) distinct from absent (`None`).
    #[test]
    fn claims_nonce_null_rejected() {
        let now = valid_now();
        let c = claims_from(json!({
            "iss": ISS, "aud": AUD, "iat": now, "jti": "j1",
            "events": { MARKER: {} }, "sub": "alice", "nonce": null,
        }));
        assert!(
            check(&c, now).is_err(),
            "present nonce key (even null) must reject"
        );
    }

    #[test]
    fn claims_nonce_absent_ok() {
        let now = valid_now();
        let c = claims_from(json!({
            "iss": ISS, "aud": AUD, "iat": now, "jti": "j1",
            "events": { MARKER: {} }, "sub": "alice",
        }));
        assert!(check(&c, now).is_ok(), "absent nonce is accepted");
    }

    #[test]
    fn claims_exp_present_rejected() {
        let now = valid_now();
        let c = claims_from(json!({
            "iss": ISS, "aud": AUD, "iat": now, "jti": "j1",
            "events": { MARKER: {} }, "sub": "alice", "exp": now + 300,
        }));
        assert!(check(&c, now).is_err(), "exp present (value) must reject");
    }

    // Same rule as nonce: a present `exp` key (even null) is forbidden on
    // logout tokens (§2.4). Double-option distinguishes present-null from absent.
    #[test]
    fn claims_exp_null_rejected() {
        let now = valid_now();
        let c = claims_from(json!({
            "iss": ISS, "aud": AUD, "iat": now, "jti": "j1",
            "events": { MARKER: {} }, "sub": "alice", "exp": null,
        }));
        assert!(
            check(&c, now).is_err(),
            "present exp key (even null) must reject"
        );
    }

    #[test]
    fn claims_exp_absent_ok() {
        let now = valid_now();
        let c = claims_from(json!({
            "iss": ISS, "aud": AUD, "iat": now, "jti": "j1",
            "events": { MARKER: {} }, "sub": "alice",
        }));
        assert!(check(&c, now).is_ok(), "absent exp is accepted");
    }

    #[test]
    fn claims_iat_freshness_window() {
        let now = valid_now();
        let mk = |iat: i64| {
            claims_from(json!({
                "iss": ISS, "aud": AUD, "iat": iat, "jti": "j1",
                "events": { MARKER: {} }, "sub": "alice",
            }))
        };
        // Stale and future beyond tolerance reject.
        assert!(check(&mk(now - 90), now).is_err(), "iat 90s stale rejects");
        assert!(check(&mk(now + 90), now).is_err(), "iat 90s future rejects");
        // Exact edges (+/-60s) are inclusive -> Ok.
        assert!(
            check(&mk(now - LOGOUT_IAT_TOLERANCE_SECS), now).is_ok(),
            "iat at -60s edge ok"
        );
        assert!(
            check(&mk(now + LOGOUT_IAT_TOLERANCE_SECS), now).is_ok(),
            "iat at +60s edge ok"
        );
    }

    #[test]
    fn claims_audience_string_and_array() {
        let now = valid_now();
        let mk = |aud: Value| {
            let mut base = json!({
                "iss": ISS, "iat": now, "jti": "j1",
                "events": { MARKER: {} }, "sub": "alice",
            });
            base.as_object_mut().unwrap().insert("aud".to_string(), aud);
            claims_from(base)
        };
        // aud missing entirely -> Aud::Missing -> reject.
        let missing = claims_from(json!({
            "iss": ISS, "iat": now, "jti": "j1",
            "events": { MARKER: {} }, "sub": "alice",
        }));
        assert!(check(&missing, now).is_err(), "missing aud rejects");
        // wrong string rejects; correct string ok.
        assert!(check(&mk(json!("someone-else")), now).is_err());
        assert!(check(&mk(json!(AUD)), now).is_ok());
        // array containing client_id ok; array not containing rejects.
        assert!(check(&mk(json!(["x", AUD, "y"])), now).is_ok());
        assert!(check(&mk(json!(["x", "y"])), now).is_err());
    }

    #[test]
    fn claims_missing_both_sub_and_sid_rejected() {
        let now = valid_now();
        let c = claims_from(json!({
            "iss": ISS, "aud": AUD, "iat": now, "jti": "j1",
            "events": { MARKER: {} },
        }));
        assert!(check(&c, now).is_err());
    }

    #[test]
    fn claims_empty_string_sub_absent_sid_is_accepted_by_policy() {
        // Locks in current behavior: check_logout_claims only tests `is_none`,
        // so an empty-string sub satisfies the "at least one of sub/sid" gate.
        // (apply_logout later filters empty strings before writing markers.)
        let now = valid_now();
        let c = claims_from(json!({
            "iss": ISS, "aud": AUD, "iat": now, "jti": "j1",
            "events": { MARKER: {} }, "sub": "",
        }));
        assert!(
            check(&c, now).is_ok(),
            "empty-string sub passes the presence gate (current behavior)"
        );
    }

    #[test]
    fn claims_events_marker_variants() {
        let now = valid_now();
        let with_events = |events: Value| {
            claims_from(json!({
                "iss": ISS, "aud": AUD, "iat": now, "jti": "j1",
                "events": events, "sub": "alice",
            }))
        };
        // marker key absent -> reject.
        assert!(check(&with_events(json!({ "other": {} })), now).is_err());
        // marker value a scalar -> reject.
        assert!(check(&with_events(json!({ MARKER: "nope" })), now).is_err());
        // marker value null -> reject.
        assert!(check(&with_events(json!({ MARKER: null })), now).is_err());
        // marker value {} -> ok.
        assert!(check(&with_events(json!({ MARKER: {} })), now).is_ok());
    }

    #[test]
    fn claims_wrong_issuer_rejected() {
        let now = valid_now();
        let c = claims_from(json!({
            "iss": "https://attacker.example.com", "aud": AUD, "iat": now, "jti": "j1",
            "events": { MARKER: {} }, "sub": "alice",
        }));
        assert!(check(&c, now).is_err());
    }

    // --- Signature-binding tests (real key fixture; JwksCache test seam) ---

    use crate::oauth::OidcClient;
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use stackpit_auth::JwksCache;

    const TEST_PRIVATE_DER: &[u8] =
        include_bytes!("../../stackpit-auth/src/testdata/test_rsa_priv.der");
    const TEST_JWKS_JSON: &str = include_str!("../../stackpit-auth/src/testdata/test_jwks.json");
    const TEST_KID: &str = "test-key-1";

    fn primed_oidc() -> OidcClient {
        let cache = JwksCache::new(
            reqwest::Client::new(),
            "http://127.0.0.1:0/jwks".to_string(),
            60,
        );
        cache._prime_raw(TEST_JWKS_JSON.to_string()).expect("prime");
        OidcClient::for_test(ISS.to_string(), AUD.to_string(), cache)
    }

    fn sign(kid: Option<&str>, claims: Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = kid.map(str::to_string);
        let key = EncodingKey::from_rsa_der(TEST_PRIVATE_DER);
        encode(&header, &claims, &key).expect("sign logout token")
    }

    #[tokio::test]
    async fn signed_valid_logout_token_ok() {
        let oidc = primed_oidc();
        let now = valid_now();
        let token = sign(
            Some(TEST_KID),
            json!({
                "iss": ISS, "aud": AUD, "iat": now, "jti": "jti-ok",
                "events": { MARKER: {} }, "sub": "alice",
            }),
        );
        let out = validate_logout_token(&oidc, &token).await;
        match out {
            LogoutValidation::Ok { sub, jti, .. } => {
                assert_eq!(sub.as_deref(), Some("alice"));
                assert_eq!(jti, "jti-ok");
            }
            LogoutValidation::Invalid => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn signed_unknown_kid_invalid() {
        let oidc = primed_oidc();
        let now = valid_now();
        // Signed with the right key, but a kid not present in the JWKS.
        let token = sign(
            Some("not-in-jwks"),
            json!({
                "iss": ISS, "aud": AUD, "iat": now, "jti": "jti-kid",
                "events": { MARKER: {} }, "sub": "alice",
            }),
        );
        let out = validate_logout_token(&oidc, &token).await;
        assert!(matches!(out, LogoutValidation::Invalid));
    }

    #[tokio::test]
    async fn signed_tampered_signature_invalid() {
        let oidc = primed_oidc();
        let now = valid_now();
        let token = sign(
            Some(TEST_KID),
            json!({
                "iss": ISS, "aud": AUD, "iat": now, "jti": "jti-tamper",
                "events": { MARKER: {} }, "sub": "alice",
            }),
        );
        // Flip the last char of the signature segment.
        let mut bytes = token.into_bytes();
        let last = bytes.len() - 1;
        bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
        let tampered = String::from_utf8(bytes).unwrap();
        let out = validate_logout_token(&oidc, &tampered).await;
        assert!(matches!(out, LogoutValidation::Invalid));
    }

    #[tokio::test]
    async fn signed_but_carries_nonce_invalid() {
        // Proves the claim layer runs after the signature passes.
        let oidc = primed_oidc();
        let now = valid_now();
        let token = sign(
            Some(TEST_KID),
            json!({
                "iss": ISS, "aud": AUD, "iat": now, "jti": "jti-nonce",
                "events": { MARKER: {} }, "sub": "alice", "nonce": "forbidden",
            }),
        );
        let out = validate_logout_token(&oidc, &token).await;
        assert!(matches!(out, LogoutValidation::Invalid));
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
}
