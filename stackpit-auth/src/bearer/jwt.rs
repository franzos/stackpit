//! JWT arm: peek unverified `iss` to pick the validator, then RS256-validate
//! against the JWKS cache. Issuer mismatch fails closed -- never falls through
//! to introspection.

use base64::Engine;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};

use super::cache::hash_token;
use super::{now_secs, BearerAuthOutcome, BearerGate, CachedResponse};

/// Clock-skew tolerance on JWT `exp` / `nbf`.
const JWT_LEEWAY_SECS: u64 = 60;

impl BearerGate {
    /// Peeks unverified `iss` to pick the validator; the validator then
    /// runs full signature + claim verification.
    pub(super) async fn authorize_jwt(
        &self,
        token: &str,
        required_scope: &str,
    ) -> BearerAuthOutcome {
        // Issuer mismatch fails closed -- never falls through to introspection.
        let Some(unverified_iss) = unverified_iss(token) else {
            tracing::warn!("bearer rejected: JWT payload missing iss");
            return BearerAuthOutcome::InvalidToken;
        };
        let expected_iss = match self.inner.expected_issuer.as_deref() {
            Some(iss) => iss,
            None => {
                tracing::warn!("bearer rejected: JWT received but no expected_issuer configured");
                return BearerAuthOutcome::InvalidToken;
            }
        };
        if unverified_iss != expected_iss {
            tracing::warn!(
                expected = %expected_iss,
                got = %unverified_iss,
                "bearer rejected: JWT issuer not configured OIDC issuer",
            );
            return BearerAuthOutcome::InvalidToken;
        }

        let Some(jwks) = self.inner.jwks.as_ref() else {
            tracing::warn!("bearer rejected: JWT received but JWKS not configured");
            return BearerAuthOutcome::InvalidToken;
        };

        // Same SHA-256 key as the opaque path: one entry per token regardless of arm.
        let token_key = hash_token(token);
        if let Some((cached, iss)) = self.cache_lookup(&token_key) {
            if self.is_revoked_cached(&iss, &cached).await {
                self.cache_evict(&token_key);
                tracing::debug!(iss = %iss, "bearer rejected: revoked (JWT cache hit path)");
                return BearerAuthOutcome::InvalidToken;
            }
            return self.check_scope(cached, iss, required_scope);
        }

        let header = match decode_header(token) {
            Ok(h) => h,
            Err(err) => {
                tracing::warn!(error = %err, "bearer rejected: JWT header undecodable");
                return BearerAuthOutcome::InvalidToken;
            }
        };
        let Some(kid) = header.kid else {
            tracing::warn!("bearer rejected: JWT header missing kid");
            return BearerAuthOutcome::InvalidToken;
        };
        // Header `alg` is informational; the validator pins RS256 below.

        let Some(jwk) = jwks.find(&kid).await else {
            tracing::warn!(kid = %kid, "bearer rejected: kid not in JWKS");
            return BearerAuthOutcome::InvalidToken;
        };
        let key = match DecodingKey::from_jwk(&jwk) {
            Ok(k) => k,
            Err(err) => {
                tracing::warn!(error = %err, "bearer rejected: JWK->DecodingKey failed");
                return BearerAuthOutcome::InvalidToken;
            }
        };

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[expected_iss]);
        if !self.inner.audience.is_empty() {
            validation.set_audience(&[self.inner.audience.as_str()]);
        } else {
            // jsonwebtoken validates aud by default; turn it off explicitly.
            validation.validate_aud = false;
        }
        validation.leeway = JWT_LEEWAY_SECS;

        let data = match decode::<serde_json::Value>(token, &key, &validation) {
            Ok(d) => d,
            Err(err) => {
                tracing::warn!(error = %err, "bearer rejected: JWT signature/claim verification failed");
                return BearerAuthOutcome::InvalidToken;
            }
        };
        let claims = data.claims;

        let Some(sub) = claims
            .get("sub")
            .and_then(|v| v.as_str())
            .map(str::to_string)
        else {
            tracing::warn!("bearer rejected: JWT missing sub");
            return BearerAuthOutcome::InvalidToken;
        };
        let sid = claims
            .get("sid")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let scope = claims
            .get("scope")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let exp = claims.get("exp").and_then(|v| v.as_i64());

        let iss = expected_iss.to_string();
        let cached = CachedResponse {
            sub: sub.clone(),
            sid,
            scope,
        };

        if self.is_revoked_cached(&iss, &cached).await {
            tracing::warn!(iss = %iss, sub = %sub, "bearer rejected: revoked (JWT)");
            return BearerAuthOutcome::InvalidToken;
        }

        let now_secs = now_secs();
        if let Some(provisioner) = &self.inner.provisioner {
            match provisioner.provision(&iss, &sub).await {
                Ok(()) => self.cache_store(token_key, &cached, &iss, exp, now_secs),
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        iss = %iss,
                        sub = %sub,
                        "JWT provisioning failed; skipping cache store"
                    );
                }
            }
        } else {
            self.cache_store(token_key, &cached, &iss, exp, now_secs);
        }

        self.check_scope(cached, iss, required_scope)
    }
}

pub(super) fn looks_like_jwt(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty())
}

/// Used only to pick the validator; signature verification re-checks `iss`.
fn unverified_iss(token: &str) -> Option<String> {
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload_b64 = parts.next()?;
    let _sig = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    json.get("iss")?.as_str().map(str::to_string)
}
