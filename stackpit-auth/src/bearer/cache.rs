//! Positive token cache + short-TTL revocation negative cache. Both are
//! SHA-256 keyed and bounded LRUs living on the [`BearerGate`] inner state.

use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::{BearerGate, CachedResponse};

pub(super) const CACHE_CAPACITY: usize = 4096;
/// Safety margin below token expiry (avoid serving token about to expire).
const CACHE_EXP_MARGIN_SECS: u64 = 5;
const REVOCATION_CACHE_TTL_SECS: u64 = 5;
pub(super) const REVOCATION_CACHE_CAPACITY: usize = 4096;

pub(super) struct CacheEntry {
    pub(super) response: CachedResponse,
    pub(super) iss: String,
    pub(super) expires_at: Instant,
}

pub(super) struct RevocationCacheEntry {
    pub(super) expires_at: Instant,
}

impl BearerGate {
    /// Short-TTL negative cache in front of [`RevocationStore`](super::RevocationStore).
    /// Only `false` answers are cached; positives bypass so revocations take
    /// effect at the next bearer-cache miss without a separate purge.
    pub(super) async fn is_revoked_cached(&self, iss: &str, cached: &CachedResponse) -> bool {
        let Some(rev) = self.inner.revocation.as_ref() else {
            return false;
        };

        let key = revocation_cache_key(iss, &cached.sub, cached.sid.as_deref());
        let now = Instant::now();

        {
            let mut cache = self.inner.revocation_cache.lock();
            match cache.get(&key) {
                Some(entry) if entry.expires_at > now => return false,
                Some(_) => {
                    cache.pop(&key);
                }
                None => {}
            }
        }

        let revoked = match rev
            .is_revoked(iss, &cached.sub, cached.sid.as_deref())
            .await
        {
            Ok(v) => v,
            Err(err) => {
                // Fail closed: a transient 401 beats serving a possibly-revoked session.
                tracing::error!(
                    error = %err,
                    iss = %iss,
                    sub = %cached.sub,
                    "revocation lookup failed; failing closed",
                );
                true
            }
        };
        if !revoked {
            self.revocation_cache_store(key, now);
        }
        revoked
    }

    fn revocation_cache_store(&self, key: [u8; 32], now: Instant) {
        let expires_at = now + Duration::from_secs(REVOCATION_CACHE_TTL_SECS);
        self.inner
            .revocation_cache
            .lock()
            .put(key, RevocationCacheEntry { expires_at });
    }

    pub(super) fn cache_evict(&self, key: &[u8; 32]) {
        if self.inner.cache_ttl.is_zero() {
            return;
        }
        self.inner.cache.lock().pop(key);
    }

    /// Drop every positive-cache entry matching `(iss, sub)`. Call alongside
    /// writing a sub-scoped row to the [`RevocationStore`](super::RevocationStore)
    /// when a user is deleted/disabled, so the *current* process stops
    /// authorizing within the cache_max_ttl window. O(n) -- cache is keyed by
    /// SHA-256(token).
    pub fn evict_sub(&self, iss: &str, sub: &str) {
        if self.inner.cache_ttl.is_zero() {
            return;
        }
        let mut cache = self.inner.cache.lock();
        let victims: Vec<[u8; 32]> = cache
            .iter()
            .filter_map(|(k, entry)| {
                if entry.iss == iss && entry.response.sub == sub {
                    Some(*k)
                } else {
                    None
                }
            })
            .collect();
        for v in victims {
            cache.pop(&v);
        }
    }

    pub(super) fn cache_lookup(&self, key: &[u8; 32]) -> Option<(CachedResponse, String)> {
        if self.inner.cache_ttl.is_zero() {
            return None;
        }
        let mut cache = self.inner.cache.lock();
        let now = Instant::now();
        match cache.get(key) {
            Some(entry) if entry.expires_at > now => {
                Some((entry.response.clone(), entry.iss.clone()))
            }
            Some(_) => {
                cache.pop(key);
                None
            }
            None => None,
        }
    }

    pub(super) fn cache_store(
        &self,
        key: [u8; 32],
        response: &CachedResponse,
        iss: &str,
        token_exp: Option<i64>,
        now_secs: i64,
    ) {
        if self.inner.cache_ttl.is_zero() {
            return;
        }

        // Cap effective TTL just under the token's `exp` so we never serve a
        // token the downstream validator would reject.
        let configured = self.inner.cache_ttl.as_secs();
        let ttl_secs = match token_exp {
            Some(exp) => {
                let lifetime = u64::try_from(exp.saturating_sub(now_secs).max(0)).unwrap_or(0);
                configured.min(lifetime.saturating_sub(CACHE_EXP_MARGIN_SECS))
            }
            None => configured,
        };
        let ttl_secs = ttl_secs.min(self.inner.cache_max_ttl.as_secs());
        if ttl_secs == 0 {
            return;
        }

        let expires_at = Instant::now() + Duration::from_secs(ttl_secs);
        self.inner.cache.lock().put(
            key,
            CacheEntry {
                response: response.clone(),
                iss: iss.to_string(),
                expires_at,
            },
        );
    }
}

pub(super) fn hash_token(token: &str) -> [u8; 32] {
    let digest = Sha256::digest(token.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn revocation_cache_key(iss: &str, sub: &str, sid: Option<&str>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    // Length-prefix prevents boundary-sliding collisions.
    hasher.update((iss.len() as u64).to_le_bytes());
    hasher.update(iss.as_bytes());
    hasher.update((sub.len() as u64).to_le_bytes());
    hasher.update(sub.as_bytes());
    match sid {
        Some(s) => {
            hasher.update([1u8]);
            hasher.update((s.len() as u64).to_le_bytes());
            hasher.update(s.as_bytes());
        }
        None => {
            hasher.update([0u8]);
        }
    }
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}
