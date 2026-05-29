//! Bearer-token dispatcher for the MCP + web surfaces.
//!
//! [`BearerGate::authorize`] dispatch:
//! 1. Size guard ([`MAX_BEARER_BYTES`]).
//! 2. admin_token (constant-time; → [`AuthContext::Admin`], bypasses scope/aud).
//! 3. JWT arm ([`jwt`]): peek unverified `iss`, require exact match against
//!    `expected_issuer`, then RS256 validate against JWKS. Issuer mismatch
//!    fails closed -- never falls through to introspection.
//! 4. Opaque arm ([`opaque`]): RFC 7662 introspection. Accepts `aud`
//!    containing the resource OR `client_id` matching the configured client
//!    (some Hydra opaque responses omit `aud`).
//!
//! Hardening:
//! - JWT alg pinned to RS256 in the validator; header `alg` never trusted.
//! - Unverified `iss` peek only selects the validator; signed `iss` is
//!   re-checked by `Validation::set_issuer`.
//! - Positive cache (SHA-256 keyed) covers both arms; revocation re-checked
//!   on hit. See [`cache`].
//!
//! Cache invariants:
//! - Cache hits do NOT re-run the provisioner. The trust anchor on hits is
//!   the [`RevocationStore`] check plus the bounded per-entry TTL.
//! - Out-of-band user deletion MUST write a sub-scoped revocation marker
//!   *before* the row delete, and should call [`BearerGate::evict_sub`] to
//!   drop in-process cache entries immediately.

mod cache;
mod jwt;
mod opaque;

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use http::HeaderMap;
use lru::LruCache;
use parking_lot::Mutex;
use secrecy::{ExposeSecret, SecretString};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::context::{AuthContext, PrincipalId};
use crate::jwks::JwksCache;

use cache::{CacheEntry, RevocationCacheEntry, CACHE_CAPACITY, REVOCATION_CACHE_CAPACITY};
use jwt::looks_like_jwt;

/// Authorization header size cap. Real JWTs are ~1 KB; opaque smaller still.
pub const MAX_BEARER_BYTES: usize = 4096;
/// Per-request introspection timeout. A hung IdP must not wedge the gate.
const DEFAULT_INTROSPECTION_TIMEOUT_SECS: u64 = 10;
/// Throttle admin_token warn-level audit logs so a credential-stuffing run
/// can't drown the signal. `debug!` still fires per hit.
const ADMIN_TOKEN_LOG_INTERVAL_SECS: u64 = 60;

/// Host hook to upsert a user row on introspection. `Err` skips the cache
/// store (next request retries) but the gate still authorizes -- provisioning
/// is a side-effect, not a trust anchor.
#[async_trait::async_trait]
pub trait UserProvisioner: Send + Sync {
    async fn provision(&self, iss: &str, sub: &str) -> ProvisionResult;
}

/// Host-hook backend failure. The gate only logs the carried message (and for
/// revocation, fails closed), so a single opaque variant covers both hooks.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("auth backend error: {0}")]
    Backend(String),
}

pub type ProvisionResult = Result<(), BackendError>;

/// Checked after every successful validation (cache hits included).
/// `Ok(true)` = revoked; `Err` = fail closed (treated as revoked).
#[async_trait::async_trait]
pub trait RevocationStore: Send + Sync {
    async fn is_revoked(
        &self,
        iss: &str,
        sub: &str,
        sid: Option<&str>,
    ) -> Result<bool, BackendError>;
}

/// Cheap to clone; Arc-shared inner state.
#[derive(Clone)]
pub struct BearerGate {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    /// `None` fails the opaque arm closed.
    introspection_url: Option<String>,
    /// Empty = skip audience check (web BFF cookie path).
    audience: String,
    /// Pinned. Unverified-`iss` mismatch fails closed before JWKS lookup.
    expected_issuer: Option<String>,
    /// Empty disables the opaque arm's `client_id` fallback.
    client_id: String,
    /// Break-glass; bypasses scope and audience.
    admin_token: Option<SecretString>,
    /// Pre-rendered HTTP Basic for the introspection POST.
    basic_auth: Option<SecretString>,
    /// `Duration::ZERO` disables caching.
    cache_ttl: Duration,
    /// Hard ceiling on any cached entry. Bounds the staleness window for IdP
    /// scope/audience rotation. `Duration::ZERO` disables caching outright.
    cache_max_ttl: Duration,
    cache: Mutex<LruCache<[u8; 32], CacheEntry>>,
    revocation_cache: Mutex<LruCache<[u8; 32], RevocationCacheEntry>>,
    /// Advertised in 401 WWW-Authenticate. Empty for non-MCP callers.
    resource_metadata_url: String,
    realm: String,
    provisioner: Option<Arc<dyn UserProvisioner>>,
    revocation: Option<Arc<dyn RevocationStore>>,
    /// `None` disables the JWT arm; opaque becomes the only path.
    jwks: Option<JwksCache>,
    /// Per-process throttle on admin_token audit warnings.
    admin_token_last_warn_secs: AtomicU64,
}

#[derive(Clone)]
struct CachedResponse {
    sub: String,
    /// Needed for sid-scoped revocation when the IdP emits it.
    sid: Option<String>,
    scope: Option<String>,
}

/// Bearer validation outcome. MCP wrapper renders 401/403; web wrapper
/// clears the grant cookie and redirects to /web/login.
pub enum BearerAuthOutcome {
    Ok(AuthContext),
    MissingToken,
    InvalidToken,
    InsufficientScope { required: String },
}

/// Caller owns the [`JwksCache`] so one cache feeds the bearer gate, the
/// id_token verifier, and the back-channel logout handler.
pub struct JwtVerifierConfig {
    pub jwks: JwksCache,
}

/// `resource_metadata_url` and `realm` are echoed in WWW-Authenticate
/// (MCP transport only; web callers can pass empty strings).
pub struct BearerGateConfig {
    /// `None` disables the opaque arm; JWT arm stays alive if `jwt` is set.
    pub introspection_url: Option<String>,
    pub audience: String,
    pub resource_metadata_url: String,
    pub realm: String,
    pub expected_issuer: Option<String>,
    /// Empty disables the opaque arm's `client_id` fallback.
    pub client_id: String,
    pub admin_token: Option<SecretString>,
    pub introspection_client_id: Option<String>,
    pub introspection_client_secret: Option<SecretString>,
    /// `0` disables caching.
    pub cache_ttl_secs: u64,
    /// Hard ceiling on any cached entry's TTL (seconds). `0` disables the
    /// positive cache.
    pub cache_max_ttl_secs: u64,
    pub provisioner: Option<Arc<dyn UserProvisioner>>,
    pub revocation: Option<Arc<dyn RevocationStore>>,
    /// `None` disables the JWT arm.
    pub jwt: Option<JwtVerifierConfig>,
}

impl BearerGate {
    /// HTTP client disables redirects (SSRF) and pins a per-request timeout
    /// so a hung introspection endpoint can't wedge the gate.
    pub fn new(cfg: BearerGateConfig) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(DEFAULT_INTROSPECTION_TIMEOUT_SECS))
            .build()?;
        Ok(Self::with_client(http, cfg))
    }

    /// Caller supplies the HTTP client (share pool, must set its own timeout).
    pub fn with_client(http: reqwest::Client, cfg: BearerGateConfig) -> Self {
        // Defense-in-depth breadcrumb; host config layer already validates this.
        if cfg.audience.is_empty() {
            tracing::warn!(
                realm = %cfg.realm,
                "bearer gate constructed without audience binding; tokens are not bound to this \
                 resource server"
            );
        }

        let basic_auth = match (cfg.introspection_client_id, cfg.introspection_client_secret) {
            (Some(id), Some(secret)) => {
                let raw = format!("{id}:{}", secret.expose_secret());
                let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
                Some(SecretString::from(format!("Basic {encoded}")))
            }
            _ => None,
        };

        let jwks = cfg.jwt.map(|j| j.jwks);

        Self {
            inner: Arc::new(Inner {
                http,
                introspection_url: cfg.introspection_url,
                audience: cfg.audience,
                expected_issuer: cfg.expected_issuer,
                client_id: cfg.client_id,
                admin_token: cfg.admin_token,
                basic_auth,
                cache_ttl: Duration::from_secs(cfg.cache_ttl_secs),
                cache_max_ttl: Duration::from_secs(cfg.cache_max_ttl_secs),
                cache: Mutex::new(LruCache::new(
                    NonZeroUsize::new(CACHE_CAPACITY).expect("CACHE_CAPACITY > 0"),
                )),
                revocation_cache: Mutex::new(LruCache::new(
                    NonZeroUsize::new(REVOCATION_CACHE_CAPACITY)
                        .expect("REVOCATION_CACHE_CAPACITY > 0"),
                )),
                resource_metadata_url: cfg.resource_metadata_url,
                realm: cfg.realm,
                provisioner: cfg.provisioner,
                revocation: cfg.revocation,
                jwks,
                admin_token_last_warn_secs: AtomicU64::new(0),
            }),
        }
    }

    /// Pass empty `required_scope` to skip the scope gate.
    pub async fn authorize(&self, token: Option<&str>, required_scope: &str) -> BearerAuthOutcome {
        let Some(token) = token.map(str::trim).filter(|s| !s.is_empty()) else {
            return BearerAuthOutcome::MissingToken;
        };

        if token.len() > MAX_BEARER_BYTES {
            tracing::warn!(len = token.len(), "bearer rejected: oversized");
            return BearerAuthOutcome::InvalidToken;
        }

        if let Some(admin) = self.inner.admin_token.as_ref() {
            if token
                .as_bytes()
                .ct_eq(admin.expose_secret().as_bytes())
                .into()
            {
                self.log_admin_break_glass();
                return BearerAuthOutcome::Ok(AuthContext::Admin);
            }
        }

        if looks_like_jwt(token) {
            return self.authorize_jwt(token, required_scope).await;
        }

        if self.inner.introspection_url.is_some() {
            return self.authorize_opaque(token, required_scope).await;
        }

        tracing::warn!("bearer rejected: not a JWT and no introspection endpoint configured");
        BearerAuthOutcome::InvalidToken
    }

    pub async fn authorize_headers(
        &self,
        headers: &HeaderMap,
        required_scope: &str,
    ) -> BearerAuthOutcome {
        self.authorize(extract_bearer(headers), required_scope)
            .await
    }

    fn check_scope(
        &self,
        cached: CachedResponse,
        iss: String,
        required_scope: &str,
    ) -> BearerAuthOutcome {
        if !required_scope.is_empty() {
            let has_scope = cached
                .scope
                .as_deref()
                .map(|s| s.split_ascii_whitespace().any(|sc| sc == required_scope))
                .unwrap_or(false);
            if !has_scope {
                tracing::warn!(
                    required = required_scope,
                    granted = cached.scope.as_deref().unwrap_or(""),
                    "bearer rejected: insufficient scope",
                );
                return BearerAuthOutcome::InsufficientScope {
                    required: required_scope.to_string(),
                };
            }
        }

        tracing::debug!(sub = %cached.sub, "bearer accepted");
        BearerAuthOutcome::Ok(AuthContext::User {
            iss,
            sub: cached.sub,
            // MCP: per-request correlation only. Web middleware swaps for
            // `PrincipalId::Session` carrying the stable grant handle.
            principal_id: PrincipalId::Request(Uuid::new_v4()),
        })
    }

    /// `warn!` at most once per [`ADMIN_TOKEN_LOG_INTERVAL_SECS`]; `debug!`
    /// per hit so high-fidelity tooling can see the full stream.
    fn log_admin_break_glass(&self) {
        let now = u64::try_from(now_secs().max(0)).unwrap_or(0);
        let last = self
            .inner
            .admin_token_last_warn_secs
            .load(Ordering::Relaxed);
        let elapsed = now.saturating_sub(last);
        if elapsed >= ADMIN_TOKEN_LOG_INTERVAL_SECS
            && self
                .inner
                .admin_token_last_warn_secs
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            tracing::warn!(
                metric = "stackpit_bearer_admin_token_used",
                "bearer accepted: admin_token break-glass (throttled; see debug for full stream)",
            );
        } else {
            tracing::debug!(
                metric = "stackpit_bearer_admin_token_used",
                "bearer accepted: admin_token break-glass",
            );
        }
    }

    /// MCP transport only; web path never calls this.
    pub fn challenge_header(&self, error: Option<&str>) -> String {
        match error {
            Some(err) => format!(
                "Bearer realm=\"{}\", error=\"{}\", resource_metadata=\"{}\"",
                self.inner.realm, err, self.inner.resource_metadata_url
            ),
            None => format!(
                "Bearer realm=\"{}\", resource_metadata=\"{}\"",
                self.inner.realm, self.inner.resource_metadata_url
            ),
        }
    }

    pub fn realm(&self) -> &str {
        &self.inner.realm
    }

    pub fn resource_metadata_url(&self) -> &str {
        &self.inner.resource_metadata_url
    }
}

/// Returns `None` if missing/malformed/empty. Guards against
/// oversized values before any string conversion.
pub fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get("authorization")?;
    if value.len() > MAX_BEARER_BYTES + "Bearer ".len() {
        return None;
    }
    value
        .to_str()
        .ok()
        // RFC 7235: the auth scheme is case-insensitive ("Bearer"/"bearer"/...).
        .and_then(|s| {
            let (scheme, token) = s.split_at_checked(7)?;
            scheme.eq_ignore_ascii_case("Bearer ").then_some(token)
        })
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Fail-closed: clock-before-epoch returns `i64::MAX` so every `exp <= now`
/// check treats the token as expired (safe degradation under skew).
fn now_secs() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        Err(err) => {
            tracing::error!(error = %err, "system clock before UNIX epoch; failing closed (now_secs = i64::MAX)");
            i64::MAX
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::jwk::JwkSet;
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde_json::json;
    use std::time::Instant;

    // Pre-generated 2048-bit RSA keypair (DER avoids needing `use_pem`).
    const TEST_PRIVATE_DER: &[u8] = include_bytes!("../testdata/test_rsa_priv.der");
    const TEST_JWKS_JSON: &str = include_str!("../testdata/test_jwks.json");
    const TEST_KID: &str = "test-key-1";

    fn jwks() -> JwkSet {
        serde_json::from_str(TEST_JWKS_JSON).expect("test JWKS parses")
    }

    fn issue_jwt(claims: serde_json::Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(TEST_KID.to_string());
        let key = EncodingKey::from_rsa_der(TEST_PRIVATE_DER);
        encode(&header, &claims, &key).expect("sign JWT")
    }

    fn base_gate(cfg_mutator: impl FnOnce(&mut BearerGateConfig)) -> BearerGate {
        let mut cfg = BearerGateConfig {
            introspection_url: None,
            audience: "https://mcp.example.com".to_string(),
            resource_metadata_url: String::new(),
            realm: "test".to_string(),
            expected_issuer: Some("https://hydra.example.com".to_string()),
            client_id: "stackpit-mcp".to_string(),
            admin_token: None,
            introspection_client_id: None,
            introspection_client_secret: None,
            cache_ttl_secs: 0,
            cache_max_ttl_secs: 30,
            provisioner: None,
            revocation: None,
            jwt: Some(JwtVerifierConfig {
                jwks: {
                    let cache = JwksCache::new(
                        reqwest::Client::new(),
                        "http://127.0.0.1:0/jwks".to_string(),
                        60,
                    );
                    cache._prime(jwks());
                    cache
                },
            }),
        };
        cfg_mutator(&mut cfg);
        BearerGate::new(cfg).expect("test HTTP client builds")
    }

    fn now() -> i64 {
        now_secs()
    }

    // Guard against `BearerGateConfig` re-deriving `Debug` later.
    #[test]
    fn secret_fields_do_not_leak_via_debug() {
        let admin = SecretString::from("dont-print-me-supersecret".to_string());
        let introspect = SecretString::from("not-this-either-clientsecret".to_string());
        let dbg = format!("{admin:?} {introspect:?}");
        assert!(
            !dbg.contains("dont-print-me-supersecret"),
            "admin_token leaked via Debug: {dbg}"
        );
        assert!(
            !dbg.contains("not-this-either-clientsecret"),
            "introspection_client_secret leaked via Debug: {dbg}"
        );
    }

    #[tokio::test]
    async fn admin_token_break_glass_hit() {
        let gate =
            base_gate(|c| c.admin_token = Some(SecretString::from("supersecret".to_string())));
        let outcome = gate.authorize(Some("supersecret"), "anything").await;
        match outcome {
            BearerAuthOutcome::Ok(AuthContext::Admin) => {}
            _ => panic!("expected admin"),
        }
    }

    #[tokio::test]
    async fn jwt_happy_path() {
        let gate = base_gate(|_| {});
        let jwt = issue_jwt(json!({
            "iss": "https://hydra.example.com",
            "sub": "alice",
            "aud": ["https://mcp.example.com"],
            "scope": "stackpit:events:read",
            "exp": now() + 300,
            "iat": now(),
        }));
        let outcome = gate.authorize(Some(&jwt), "stackpit:events:read").await;
        match outcome {
            BearerAuthOutcome::Ok(AuthContext::User { sub, iss, .. }) => {
                assert_eq!(sub, "alice");
                assert_eq!(iss, "https://hydra.example.com");
            }
            _ => panic!("expected user"),
        }
    }

    #[tokio::test]
    async fn jwt_wrong_issuer_fails_closed() {
        let gate = base_gate(|_| {});
        let jwt = issue_jwt(json!({
            "iss": "https://attacker.example.com",
            "sub": "alice",
            "aud": ["https://mcp.example.com"],
            "exp": now() + 300,
        }));
        let outcome = gate.authorize(Some(&jwt), "").await;
        assert!(matches!(outcome, BearerAuthOutcome::InvalidToken));
    }

    #[tokio::test]
    async fn jwt_missing_audience_rejected() {
        let gate = base_gate(|_| {});
        let jwt = issue_jwt(json!({
            "iss": "https://hydra.example.com",
            "sub": "alice",
            "aud": ["https://other.example.com"],
            "exp": now() + 300,
        }));
        let outcome = gate.authorize(Some(&jwt), "").await;
        assert!(matches!(outcome, BearerAuthOutcome::InvalidToken));
    }

    #[tokio::test]
    async fn jwt_expired_rejected() {
        let gate = base_gate(|_| {});
        let jwt = issue_jwt(json!({
            "iss": "https://hydra.example.com",
            "sub": "alice",
            "aud": ["https://mcp.example.com"],
            "exp": now() - 3600,
        }));
        let outcome = gate.authorize(Some(&jwt), "").await;
        assert!(matches!(outcome, BearerAuthOutcome::InvalidToken));
    }

    #[tokio::test]
    async fn jwt_insufficient_scope() {
        let gate = base_gate(|_| {});
        let jwt = issue_jwt(json!({
            "iss": "https://hydra.example.com",
            "sub": "alice",
            "aud": ["https://mcp.example.com"],
            "scope": "openid",
            "exp": now() + 300,
        }));
        let outcome = gate.authorize(Some(&jwt), "stackpit:events:read").await;
        match outcome {
            BearerAuthOutcome::InsufficientScope { required } => {
                assert_eq!(required, "stackpit:events:read");
            }
            _ => panic!("expected insufficient_scope"),
        }
    }

    #[tokio::test]
    async fn jwt_bad_signature_rejected() {
        let gate = base_gate(|_| {});
        let mut jwt = issue_jwt(json!({
            "iss": "https://hydra.example.com",
            "sub": "alice",
            "aud": ["https://mcp.example.com"],
            "exp": now() + 300,
        }));
        // Flip a character inside the signature segment.
        let last_dot = jwt.rfind('.').unwrap();
        let sig_start = last_dot + 1;
        let mut bytes = jwt.into_bytes();
        let target = bytes[sig_start];
        // Swap to a definitely-different valid base64url char.
        bytes[sig_start] = if target == b'A' { b'B' } else { b'A' };
        jwt = String::from_utf8(bytes).unwrap();
        let outcome = gate.authorize(Some(&jwt), "").await;
        assert!(matches!(outcome, BearerAuthOutcome::InvalidToken));
    }

    #[tokio::test]
    async fn size_guard_oversized_token() {
        let gate = base_gate(|_| {});
        let huge = "x".repeat(MAX_BEARER_BYTES + 1);
        let outcome = gate.authorize(Some(&huge), "").await;
        assert!(matches!(outcome, BearerAuthOutcome::InvalidToken));
    }

    #[tokio::test]
    async fn opaque_with_no_introspection_url_rejected() {
        let gate = base_gate(|c| c.introspection_url = None);
        let outcome = gate.authorize(Some("opaque-token-string"), "").await;
        assert!(matches!(outcome, BearerAuthOutcome::InvalidToken));
    }

    #[tokio::test]
    async fn missing_token_returns_missing() {
        let gate = base_gate(|_| {});
        let outcome = gate.authorize(None, "").await;
        assert!(matches!(outcome, BearerAuthOutcome::MissingToken));
    }

    // [C2] At capacity, LRU evicts oldest; touched entry survives.
    #[tokio::test]
    async fn cache_lru_evicts_least_recently_used() {
        let gate = base_gate(|c| c.cache_ttl_secs = 10);
        let response = CachedResponse {
            sub: "alice".to_string(),
            sid: None,
            scope: None,
        };
        let now = now_secs();
        // Fill cache to capacity.
        for i in 0..CACHE_CAPACITY {
            let mut key = [0u8; 32];
            key[..8].copy_from_slice(&(i as u64).to_le_bytes());
            gate.cache_store(key, &response, "iss", Some(now + 3600), now);
        }
        assert_eq!(gate.inner.cache.lock().len(), CACHE_CAPACITY);

        // Ensure the touch is strictly later than every fill insert.
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Touch entry 0 so it becomes most-recently-used.
        let mut touched = [0u8; 32];
        touched[..8].copy_from_slice(&0u64.to_le_bytes());
        let hit = gate.cache_lookup(&touched);
        assert!(hit.is_some(), "entry 0 should still be present");

        // Insert a new entry; some non-touched entry must be evicted, never entry 0.
        let mut new_key = [0u8; 32];
        new_key[..8].copy_from_slice(&(CACHE_CAPACITY as u64).to_le_bytes());
        gate.cache_store(new_key, &response, "iss", Some(now + 3600), now);

        let cache = gate.inner.cache.lock();
        assert_eq!(cache.len(), CACHE_CAPACITY);
        assert!(
            cache.contains(&touched),
            "touched entry 0 must survive (MRU)"
        );
        assert!(cache.contains(&new_key), "new entry inserted");
    }

    // [C6] cache_store caps TTL at the configured ceiling.
    #[tokio::test]
    async fn cache_store_caps_ttl_at_max() {
        let gate = base_gate(|c| {
            c.cache_ttl_secs = 600;
            c.cache_max_ttl_secs = 5;
        });
        let key = [42u8; 32];
        let response = CachedResponse {
            sub: "alice".to_string(),
            sid: None,
            scope: None,
        };
        let now = now_secs();
        gate.cache_store(key, &response, "iss", Some(now + 3600), now);
        let cache = gate.inner.cache.lock();
        let entry = cache.peek(&key).expect("entry stored");
        let remaining = entry.expires_at.saturating_duration_since(Instant::now());
        assert!(
            remaining <= Duration::from_secs(5),
            "ttl must be capped at 5s, got {remaining:?}"
        );
    }

    // Ceiling 0 disables the positive cache.
    #[tokio::test]
    async fn cache_max_ttl_zero_disables_cache() {
        let gate = base_gate(|c| {
            c.cache_ttl_secs = 60;
            c.cache_max_ttl_secs = 0;
        });
        let key = [7u8; 32];
        let response = CachedResponse {
            sub: "alice".to_string(),
            sid: None,
            scope: None,
        };
        let now = now_secs();
        gate.cache_store(key, &response, "iss", Some(now + 3600), now);
        assert!(
            gate.cache_lookup(&key).is_none(),
            "ceiling=0 must short-circuit cache_store"
        );
    }

    // [H9] Revocation cache hits within TTL skip the backing store.
    struct CountingRevocation {
        calls: Mutex<u32>,
    }

    #[async_trait::async_trait]
    impl RevocationStore for CountingRevocation {
        async fn is_revoked(
            &self,
            _iss: &str,
            _sub: &str,
            _sid: Option<&str>,
        ) -> Result<bool, BackendError> {
            *self.calls.lock() += 1;
            Ok(false)
        }
    }

    #[tokio::test]
    async fn revocation_negative_cached_within_ttl() {
        let counter = Arc::new(CountingRevocation {
            calls: Mutex::new(0),
        });
        let gate = base_gate(|c| {
            c.cache_ttl_secs = 60;
            c.revocation = Some(counter.clone());
        });
        let response = CachedResponse {
            sub: "alice".to_string(),
            sid: None,
            scope: None,
        };
        let _ = gate.is_revoked_cached("iss", &response).await;
        let _ = gate.is_revoked_cached("iss", &response).await;
        let _ = gate.is_revoked_cached("iss", &response).await;
        assert_eq!(
            *counter.calls.lock(),
            1,
            "second + third lookup should hit the negative cache"
        );
    }
}
