//! JWKS fetch + cache, shared across the MCP JWT arm, id_token verifier,
//! and back-channel logout verifier.
//!
//! One `JwkSet` per provider, TTL-bounded. `kid` miss or expired TTL
//! triggers refetch; transient failure leaves the previous set intact
//! (stale-but-serving while the IdP recovers).

use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use tokio::sync::Mutex as AsyncMutex;

/// A slow JWKS endpoint must not stall every JWT validation.
const JWKS_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Hard cap on the JWKS response body so a hostile IdP can't stream us into OOM.
const JWKS_MAX_BODY_BYTES: usize = 1 << 20;

/// Defense-in-depth TTL floor (host validates at startup; this catches
/// library users building `JwksCache` directly).
const JWKS_TTL_FLOOR_SECS: u64 = 60;

/// Shared between `prime` and the internal refetch path so the discovery
/// surface can decide whether a startup failure is fatal.
#[derive(Debug)]
pub enum JwksError {
    Request(String),
    Status(u16),
    Body(String),
    Parse(String),
}

impl std::fmt::Display for JwksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JwksError::Request(e) => write!(f, "JWKS fetch request failed: {e}"),
            JwksError::Status(s) => write!(f, "JWKS endpoint returned non-2xx status: {s}"),
            JwksError::Body(e) => write!(f, "JWKS response body unreadable: {e}"),
            JwksError::Parse(e) => write!(f, "JWKS response did not parse: {e}"),
        }
    }
}

impl std::error::Error for JwksError {}

/// Failure modes of [`JwksCache::verify_rs256`]. Distinguishes "couldn't even
/// parse the JWT" from "no key for kid" from "signature/claims rejected" so
/// callers can log precisely without leaking detail to the client.
#[derive(Debug)]
pub enum VerifyError {
    /// JWT header undecodable.
    MalformedHeader(String),
    /// Header `alg` is not RS256.
    UnexpectedAlg(String),
    /// Header carries no `kid`.
    MissingKid,
    /// No JWK in the cache matches the header `kid`.
    UnknownKid(String),
    /// The matched JWK couldn't be turned into a verifying key.
    BadKey(String),
    /// Signature or claim validation failed.
    Validation(String),
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::MalformedHeader(e) => write!(f, "malformed JWT header: {e}"),
            VerifyError::UnexpectedAlg(a) => write!(f, "JWT alg {a} is not RS256"),
            VerifyError::MissingKid => write!(f, "JWT header missing kid"),
            VerifyError::UnknownKid(kid) => write!(f, "no JWK matches kid {kid}"),
            VerifyError::BadKey(e) => write!(f, "JWK -> decoding key failed: {e}"),
            VerifyError::Validation(e) => write!(f, "JWT signature/claim validation failed: {e}"),
        }
    }
}

impl std::error::Error for VerifyError {}

struct CacheState {
    keys: JwkSet,
    /// Verbatim JWKS body. Callers needing another JWK representation
    /// (e.g. openidconnect's `CoreJsonWebKey`) re-parse this.
    raw: String,
    fetched_at: Instant,
}

/// Async JWKS fetcher with a TTL cache and refetch-on-miss. Cheap to clone.
#[derive(Clone)]
pub struct JwksCache {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    jwks_url: String,
    ttl: Duration,
    state: Mutex<Option<CacheState>>,
    refetch_lock: AsyncMutex<()>,
}

impl JwksCache {
    pub fn new(http: reqwest::Client, jwks_url: String, ttl_secs: u64) -> Self {
        let effective_ttl_secs = if ttl_secs < JWKS_TTL_FLOOR_SECS {
            tracing::warn!(
                configured = ttl_secs,
                floor = JWKS_TTL_FLOOR_SECS,
                url = %jwks_url,
                "jwks_cache_ttl_secs below floor; clamping to floor to avoid refetch-per-request \
                 self-DoS. Set a value >= floor to silence this warning.",
            );
            JWKS_TTL_FLOOR_SECS
        } else {
            ttl_secs
        };
        Self {
            inner: Arc::new(Inner {
                http,
                jwks_url,
                ttl: Duration::from_secs(effective_ttl_secs),
                state: Mutex::new(None),
                refetch_lock: AsyncMutex::new(()),
            }),
        }
    }

    pub fn jwks_url(&self) -> &str {
        &self.inner.jwks_url
    }

    /// Hits cache first; refetches past TTL or on `kid` miss. Refetch
    /// failure falls back to whatever the cache still holds.
    pub async fn find(&self, kid: &str) -> Option<Jwk> {
        if self.needs_refetch(kid) {
            let _ = self.refetch(kid).await;
        }
        self.lookup(|s| jwk_with_kid(&s.keys, kid))
    }

    /// Verbatim JWKS body for `kid` (refetches on miss). For callers that
    /// need a different JWK representation than `jsonwebtoken::jwk::Jwk`.
    pub async fn raw_for_kid(&self, kid: &str) -> Option<String> {
        if self.needs_refetch(kid) {
            let _ = self.refetch(kid).await;
        }
        self.lookup(|s| has_kid(&s.keys, kid).then(|| s.raw.clone()))
    }

    /// Shared RS256 primitive: resolve the signing key from the JWT's `kid`
    /// (refetching on miss), pin the algorithm to RS256, and decode + verify
    /// the signature and the claims expressed in `validation` (issuer,
    /// audience, exp, ...). Returns the deserialized claims.
    ///
    /// The validator is caller-supplied so each token kind keeps its own
    /// claim policy -- e.g. back-channel logout tokens disable `exp` and apply
    /// their spec-specific checks on top of the returned claims. Pinning the
    /// validator's algorithm to RS256 is the caller's job; the header `alg`
    /// here is only checked to fail fast before the JWKS lookup.
    pub async fn verify_rs256<T: DeserializeOwned>(
        &self,
        token: &str,
        validation: &Validation,
    ) -> Result<T, VerifyError> {
        let header =
            decode_header(token).map_err(|e| VerifyError::MalformedHeader(e.to_string()))?;
        if header.alg != Algorithm::RS256 {
            return Err(VerifyError::UnexpectedAlg(format!("{:?}", header.alg)));
        }
        let kid = header.kid.ok_or(VerifyError::MissingKid)?;
        let jwk = self
            .find(&kid)
            .await
            .ok_or_else(|| VerifyError::UnknownKid(kid.clone()))?;
        let key = DecodingKey::from_jwk(&jwk).map_err(|e| VerifyError::BadKey(e.to_string()))?;
        decode::<T>(token, &key, validation)
            .map(|data| data.claims)
            .map_err(|e| VerifyError::Validation(e.to_string()))
    }

    fn needs_refetch(&self, kid: &str) -> bool {
        let state = self.inner.state.lock();
        match state.as_ref() {
            None => true,
            Some(entry) => {
                entry.fetched_at.elapsed() > self.inner.ttl || !has_kid(&entry.keys, kid)
            }
        }
    }

    fn lookup<T>(&self, project: impl FnOnce(&CacheState) -> Option<T>) -> Option<T> {
        let state = self.inner.state.lock();
        let entry = state.as_ref()?;
        project(entry)
    }

    /// Single-flighted: concurrent callers queue, then re-check the cache
    /// after the leader returns. Transient failures keep the existing
    /// entry; warn-logging happens here.
    async fn refetch(&self, kid: &str) -> Result<(), ()> {
        let _guard = self.inner.refetch_lock.lock().await;
        if !self.needs_refetch(kid) {
            return Ok(());
        }
        self.do_fetch().await.map_err(|err| {
            tracing::warn!(error = %err, url = %self.inner.jwks_url, "JWKS fetch failed");
        })
    }

    /// Warm the cache so the first JWT request skips the RTT. Surfaces the
    /// error type (unlike the lazy refetch path) so callers can fail fast
    /// when OAuth is `required`.
    pub async fn prime(&self) -> Result<(), JwksError> {
        let _guard = self.inner.refetch_lock.lock().await;
        self.do_fetch().await
    }

    async fn do_fetch(&self) -> Result<(), JwksError> {
        let mut resp = self
            .inner
            .http
            .get(&self.inner.jwks_url)
            .timeout(JWKS_FETCH_TIMEOUT)
            .send()
            .await
            .map_err(|err| JwksError::Request(err.to_string()))?;
        if !resp.status().is_success() {
            return Err(JwksError::Status(resp.status().as_u16()));
        }
        // Short-circuit if the IdP declared a body too large to be a JWKS.
        if let Some(len) = resp.content_length() {
            if len > JWKS_MAX_BODY_BYTES as u64 {
                return Err(JwksError::Body(format!(
                    "JWKS Content-Length {len} exceeds cap {JWKS_MAX_BODY_BYTES}"
                )));
            }
        }
        // Stream-with-limit bounds bytes even when the server lies about Content-Length.
        let mut buf = Vec::with_capacity(8 * 1024);
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    if buf.len() + chunk.len() > JWKS_MAX_BODY_BYTES {
                        return Err(JwksError::Body(format!(
                            "JWKS response exceeds cap {JWKS_MAX_BODY_BYTES} bytes"
                        )));
                    }
                    buf.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(err) => return Err(JwksError::Body(err.to_string())),
            }
        }
        let body = String::from_utf8(buf).map_err(|err| JwksError::Body(err.to_string()))?;
        let keys: JwkSet =
            serde_json::from_str(&body).map_err(|err| JwksError::Parse(err.to_string()))?;
        *self.inner.state.lock() = Some(CacheState {
            keys,
            raw: body,
            fetched_at: Instant::now(),
        });
        Ok(())
    }

    /// Test hook: bypasses the TTL floor. Use [`Self::new`] in production.
    #[doc(hidden)]
    pub fn _new_unchecked_ttl(http: reqwest::Client, jwks_url: String, ttl_secs: u64) -> Self {
        Self {
            inner: Arc::new(Inner {
                http,
                jwks_url,
                ttl: Duration::from_secs(ttl_secs.max(1)),
                state: Mutex::new(None),
                refetch_lock: AsyncMutex::new(()),
            }),
        }
    }

    /// Test hook: prime the cache without a network call.
    #[doc(hidden)]
    pub fn _prime_raw(&self, raw: String) -> Result<(), serde_json::Error> {
        let keys: JwkSet = serde_json::from_str(&raw)?;
        *self.inner.state.lock() = Some(CacheState {
            keys,
            raw,
            fetched_at: Instant::now(),
        });
        Ok(())
    }

    /// Test hook: prime the cache without a network call.
    #[doc(hidden)]
    pub fn _prime(&self, keys: JwkSet) {
        let raw = serde_json::to_string(&keys).unwrap_or_default();
        *self.inner.state.lock() = Some(CacheState {
            keys,
            raw,
            fetched_at: Instant::now(),
        });
    }
}

fn jwk_with_kid(set: &JwkSet, kid: &str) -> Option<Jwk> {
    set.keys
        .iter()
        .find(|k| k.common.key_id.as_deref() == Some(kid))
        .cloned()
}

fn has_kid(set: &JwkSet, kid: &str) -> bool {
    set.keys
        .iter()
        .any(|k| k.common.key_id.as_deref() == Some(kid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// In-process server flips the JWKS `kid` after the first request --
    /// proves refetch fires on kid miss.
    #[tokio::test]
    async fn refetch_on_kid_miss_returns_rotated_key() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        const JWKS_OLD: &str = r#"{"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":"old","n":"AQAB","e":"AQAB"}]}"#;
        const JWKS_NEW: &str = r#"{"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":"new","n":"AQAB","e":"AQAB"}]}"#;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_srv = hits.clone();

        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let hits = hits_srv.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf).await;
                    let n = hits.fetch_add(1, Ordering::SeqCst);
                    let body = if n == 0 { JWKS_OLD } else { JWKS_NEW };
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });

        let cache = JwksCache::new(reqwest::Client::new(), format!("http://{addr}/jwks"), 3600);

        // OLD populates the cache.
        assert!(cache.find("old").await.is_some());
        // `new` triggers refetch; rotated JWKS replaces the set wholesale.
        assert!(cache.find("new").await.is_some());
        assert!(cache.find("old").await.is_none());

        assert!(
            hits.load(Ordering::SeqCst) >= 2,
            "expected at least two HTTP hits"
        );
    }

    /// `prime` warms the cache; follow-up `find` must hit zero HTTP times.
    #[tokio::test]
    async fn prime_warms_cache_one_hit() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        const JWKS_BODY: &str = r#"{"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":"k1","n":"AQAB","e":"AQAB"}]}"#;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_srv = hits.clone();

        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let hits = hits_srv.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf).await;
                    hits.fetch_add(1, Ordering::SeqCst);
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        JWKS_BODY.len(),
                        JWKS_BODY
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });

        let cache = JwksCache::new(reqwest::Client::new(), format!("http://{addr}/jwks"), 3600);
        cache.prime().await.expect("prime succeeds");
        assert!(cache.find("k1").await.is_some());
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "prime should warm the cache so find() hits zero times"
        );
    }

    /// `prime` surfaces a typed error so discovery can fail-fast.
    #[tokio::test]
    async fn prime_surfaces_error_on_failure() {
        let cache = JwksCache::new(
            reqwest::Client::new(),
            "http://127.0.0.1:1/jwks".to_string(),
            60,
        );
        let err = cache.prime().await.expect_err("closed port must fail");
        assert!(matches!(err, JwksError::Request(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn raw_for_kid_returns_body_on_match() {
        let cache = JwksCache::new(
            reqwest::Client::new(),
            "http://127.0.0.1:0/jwks".to_string(),
            60,
        );
        let raw = r#"{"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":"k1","n":"AQAB","e":"AQAB"}]}"#;
        cache._prime_raw(raw.to_string()).unwrap();
        assert_eq!(cache.raw_for_kid("k1").await.as_deref(), Some(raw));
        // Miss against a primed cache with no server reachable returns None.
        assert!(cache.raw_for_kid("missing").await.is_none());
    }

    /// Known kids must survive a transient JWKS endpoint outage.
    #[tokio::test]
    async fn keep_stale_on_refetch_failure() {
        // 127.0.0.1:1 is unbound -- guaranteed connection refused.
        let cache = JwksCache::_new_unchecked_ttl(
            reqwest::Client::new(),
            "http://127.0.0.1:1/jwks".to_string(),
            // Clamped to 1s so the next find() takes the refetch path.
            1,
        );
        let raw = r#"{"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":"k1","n":"AQAB","e":"AQAB"}]}"#;
        cache._prime_raw(raw.to_string()).unwrap();

        // Force the entry past TTL so find() will try to refetch.
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // Refetch hits a closed port and fails; the cached `k1` must remain.
        assert!(
            cache.find("k1").await.is_some(),
            "primed kid should survive a transient refetch failure",
        );
        assert_eq!(
            cache.raw_for_kid("k1").await.as_deref(),
            Some(raw),
            "raw body should also survive a transient refetch failure",
        );
    }
}
