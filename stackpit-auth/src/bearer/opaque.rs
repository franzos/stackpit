//! RFC 7662 introspection arm. Accepts `aud` containing the resource OR
//! `client_id` matching the configured client (some Hydra opaque responses
//! omit `aud`).

use secrecy::ExposeSecret;
use serde::Deserialize;

use super::cache::hash_token;
use super::{now_secs, BearerAuthOutcome, BearerGate, CachedResponse};

/// RFC 7662 fields we read; Hydra returns more.
#[derive(Debug, Deserialize)]
struct IntrospectionResponse {
    #[serde(default)]
    active: bool,
    #[serde(default)]
    sub: Option<String>,
    #[serde(default)]
    aud: Aud,
    #[serde(default)]
    scope: Option<String>,
    /// Unix seconds.
    #[serde(default)]
    exp: Option<i64>,
    #[serde(default)]
    iss: Option<String>,
    #[serde(default)]
    sid: Option<String>,
    /// RFC 7662 §2.2. Opaque path accepts this as an `aud` alternative.
    #[serde(default)]
    client_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(untagged)]
enum Aud {
    #[default]
    None,
    One(String),
    Many(Vec<String>),
}

impl Aud {
    fn contains(&self, expected: &str) -> bool {
        match self {
            Aud::None => false,
            Aud::One(s) => s == expected,
            Aud::Many(v) => v.iter().any(|a| a == expected),
        }
    }
}

impl BearerGate {
    /// RFC 7662 introspection arm.
    pub(super) async fn authorize_opaque(
        &self,
        token: &str,
        required_scope: &str,
    ) -> BearerAuthOutcome {
        let token_key = hash_token(token);

        if let Some((cached, iss)) = self.cache_lookup(&token_key) {
            if self.is_revoked_cached(&iss, &cached).await {
                self.cache_evict(&token_key);
                tracing::debug!(iss = %iss, "bearer rejected: revoked (opaque cache hit path)");
                return BearerAuthOutcome::InvalidToken;
            }
            return self.check_scope(cached, iss, required_scope);
        }

        let body = match self.introspect(token).await {
            Ok(body) => body,
            Err(()) => return BearerAuthOutcome::InvalidToken,
        };

        if !body.active {
            tracing::debug!("bearer rejected: active=false");
            return BearerAuthOutcome::InvalidToken;
        }

        // Some Hydra opaque tokens omit `aud`; accept `client_id` match instead.
        if !self.inner.audience.is_empty() {
            let aud_match = body.aud.contains(&self.inner.audience);
            let client_id_match = !self.inner.client_id.is_empty()
                && body
                    .client_id
                    .as_deref()
                    .map(|c| c == self.inner.client_id)
                    .unwrap_or(false);
            if !aud_match && !client_id_match {
                tracing::warn!(
                    expected_aud = %self.inner.audience,
                    expected_client = %self.inner.client_id,
                    "bearer rejected: opaque aud/client_id mismatch",
                );
                return BearerAuthOutcome::InvalidToken;
            }
        }

        if let Some(expected) = &self.inner.expected_issuer {
            match &body.iss {
                Some(actual) if actual == expected => { /* ok */ }
                Some(actual) => {
                    tracing::warn!(
                        expected = %expected,
                        actual = %actual,
                        "bearer rejected: opaque iss mismatch",
                    );
                    return BearerAuthOutcome::InvalidToken;
                }
                None => {
                    // Some Hydra setups omit iss; admin URL is the trust anchor.
                }
            }
        }

        let now = now_secs();
        if let Some(exp) = body.exp {
            if exp <= now {
                tracing::debug!(exp, now, "bearer rejected: opaque token expired");
                return BearerAuthOutcome::InvalidToken;
            }
        }

        let Some(sub) = body.sub.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
            tracing::warn!("bearer rejected: introspection response missing sub");
            return BearerAuthOutcome::InvalidToken;
        };
        let sub = sub.to_string();

        // Empty `iss` would collapse to a global cache/revocation key; refuse.
        let iss = body
            .iss
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                self.inner
                    .expected_issuer
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            });
        let Some(iss) = iss else {
            tracing::warn!(
                "bearer rejected: no resolvable non-empty issuer (introspection omitted iss and no expected_issuer configured)"
            );
            return BearerAuthOutcome::InvalidToken;
        };
        let iss = iss.to_string();

        let cached = CachedResponse {
            sub: sub.clone(),
            sid: body.sid.clone(),
            scope: body.scope.clone(),
        };

        if self.is_revoked_cached(&iss, &cached).await {
            tracing::warn!(iss = %iss, sub = %sub, "bearer rejected: revoked (opaque)");
            return BearerAuthOutcome::InvalidToken;
        }

        if let Some(provisioner) = &self.inner.provisioner {
            match provisioner.provision(&iss, &sub).await {
                Ok(()) => self.cache_store(token_key, &cached, &iss, body.exp, now),
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        iss = %iss,
                        sub = %sub,
                        "opaque provisioning failed; skipping cache store"
                    );
                }
            }
        } else {
            self.cache_store(token_key, &cached, &iss, body.exp, now);
        }

        self.check_scope(cached, iss, required_scope)
    }

    /// Errors don't leak to client.
    async fn introspect(&self, token: &str) -> Result<IntrospectionResponse, ()> {
        let url = self.inner.introspection_url.as_deref().ok_or(())?;
        let mut req = self.inner.http.post(url).form(&[("token", token)]);
        if let Some(auth) = self.inner.basic_auth.as_ref() {
            req = req.header(reqwest::header::AUTHORIZATION, auth.expose_secret());
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(err) => {
                tracing::warn!(error = %err, "introspection request failed");
                return Err(());
            }
        };

        if !resp.status().is_success() {
            tracing::warn!(status = %resp.status(), "introspection returned non-2xx");
            return Err(());
        }

        resp.json::<IntrospectionResponse>().await.map_err(|err| {
            tracing::warn!(error = %err, "introspection returned unparseable JSON");
        })
    }
}
