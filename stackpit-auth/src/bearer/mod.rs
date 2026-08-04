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
//! - Short-TTL negative cache on the opaque arm: definitive introspection
//!   rejections are remembered so forged tokens can't drive one POST each.
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
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use http::HeaderMap;
use lru::LruCache;
use parking_lot::Mutex;
use secrecy::{ExposeSecret, SecretString};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::context::{AuthContext, PrincipalId};
use crate::jwks::JwksCache;

use cache::{
    CacheEntry, RevocationCacheEntry, CACHE_CAPACITY, NEGATIVE_CACHE_CAPACITY,
    REVOCATION_CACHE_CAPACITY,
};
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
    /// Short-TTL negative cache of definitively rejected opaque tokens
    /// (SHA-256 keyed); expiry per entry. See [`cache`].
    negative_cache: Mutex<LruCache<[u8; 32], Instant>>,
    /// Advertised in 401 WWW-Authenticate. Empty for non-MCP callers.
    resource_metadata_url: String,
    /// Space-delimited `scope` for the 401 challenge. Empty omits it.
    challenge_scope: String,
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
    /// OAuth client that presented the token. Join key for per-client policy
    /// and the "which app did this" field in audit events.
    client_id: Option<String>,
}

/// Scopes carried by the presented token, already split. Inserted as a request
/// extension by [`mcp_auth_middleware`](crate::mcp_auth_middleware); the cookie
/// path never has one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrantedScopes(Vec<String>);

impl GrantedScopes {
    pub fn parse(raw: Option<&str>) -> Self {
        Self(
            raw.unwrap_or_default()
                .split_ascii_whitespace()
                .map(str::to_string)
                .collect(),
        )
    }

    pub fn has(&self, scope: &str) -> bool {
        self.0.iter().any(|s| s == scope)
    }

    pub fn as_slice(&self) -> &[String] {
        &self.0
    }
}

/// OAuth `client_id` of the token presenter. Request extension. `None` when the
/// credential names no client: the admin-token break-glass, or an introspection
/// response that omits the field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenClientId(pub Option<String>);

/// An authenticated bearer plus the token facts a resource server needs to
/// authorize per-tool. [`AuthContext`] stays narrow because the cookie path
/// shares it and scopes are meaningless there.
pub struct BearerGrant {
    pub ctx: AuthContext,
    pub scopes: GrantedScopes,
    pub client_id: TokenClientId,
}

/// Bearer validation outcome. MCP wrapper renders 401/403; web wrapper
/// clears the grant cookie and redirects to /web/login.
pub enum BearerAuthOutcome {
    Ok(BearerGrant),
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
    /// Space-delimited scopes a token-less caller should request. Must be the
    /// set the resource metadata advertises; empty omits `scope` from the
    /// challenge.
    pub challenge_scope: String,
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
                negative_cache: Mutex::new(LruCache::new(
                    NonZeroUsize::new(NEGATIVE_CACHE_CAPACITY)
                        .expect("NEGATIVE_CACHE_CAPACITY > 0"),
                )),
                resource_metadata_url: cfg.resource_metadata_url,
                challenge_scope: cfg.challenge_scope,
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
                return BearerAuthOutcome::Ok(BearerGrant {
                    ctx: AuthContext::Admin,
                    scopes: GrantedScopes::default(),
                    client_id: TokenClientId(None),
                });
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
        BearerAuthOutcome::Ok(BearerGrant {
            ctx: AuthContext::User {
                iss,
                sub: cached.sub,
                // MCP: per-request correlation only. Web middleware swaps for
                // `PrincipalId::Session` carrying the stable grant handle.
                principal_id: PrincipalId::Request(Uuid::new_v4()),
            },
            scopes: GrantedScopes::parse(cached.scope.as_deref()),
            client_id: TokenClientId(cached.client_id),
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

    /// MCP transport only; web path never calls this. RFC 6750 §3: `scope`
    /// belongs on the challenge whenever the gate knows which scope was
    /// insufficient -- it is what tells the client what to step up to.
    pub fn challenge_header(&self, error: Option<&str>, scope: Option<&str>) -> String {
        let mut challenge = format!("Bearer realm=\"{}\"", self.inner.realm);
        if let Some(err) = error {
            challenge.push_str(&format!(", error=\"{err}\""));
        }
        if let Some(scope) = scope.filter(|s| !s.is_empty()) {
            challenge.push_str(&format!(", scope=\"{scope}\""));
        }
        challenge.push_str(&format!(
            ", resource_metadata=\"{}\"",
            self.inner.resource_metadata_url
        ));
        challenge
    }

    pub fn realm(&self) -> &str {
        &self.inner.realm
    }

    pub fn resource_metadata_url(&self) -> &str {
        &self.inner.resource_metadata_url
    }

    /// What a caller holding no usable token should ask the IdP for.
    pub fn challenge_scope(&self) -> &str {
        &self.inner.challenge_scope
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
            challenge_scope: String::new(),
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
            BearerAuthOutcome::Ok(BearerGrant {
                ctx: AuthContext::Admin,
                ..
            }) => {}
            _ => panic!("expected admin"),
        }
    }

    /// `build_mcp_runtime` leaves `admin_token` unset so `/mcp` honours MCP's
    /// rule that a resource server accepts only tokens its authorization server
    /// issued for it. Without the break-glass wired in there is no admin arm.
    #[tokio::test]
    async fn no_admin_token_configured_leaves_no_break_glass_arm() {
        let gate = base_gate(|c| c.admin_token = None);
        match gate.authorize(Some("supersecret"), "anything").await {
            BearerAuthOutcome::InvalidToken => {}
            _ => panic!("expected InvalidToken"),
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
            BearerAuthOutcome::Ok(BearerGrant { ctx, scopes, .. }) => {
                let AuthContext::User { sub, iss, .. } = ctx else {
                    panic!("expected user")
                };
                assert_eq!(sub, "alice");
                assert_eq!(iss, "https://hydra.example.com");
                assert!(scopes.has("stackpit:events:read"));
            }
            _ => panic!("expected user"),
        }
    }

    // The join key for per-client policy and for "which app did this" in an
    // audit line; Hydra sets it and the extractor used to drop it.
    #[tokio::test]
    async fn jwt_carries_the_client_id_through() {
        let gate = base_gate(|_| {});
        let jwt = issue_jwt(json!({
            "iss": "https://hydra.example.com",
            "sub": "alice",
            "aud": ["https://mcp.example.com"],
            "scope": "stackpit:events:read",
            "client_id": "mcp-client",
            "exp": now() + 300,
        }));
        match gate.authorize(Some(&jwt), "").await {
            BearerAuthOutcome::Ok(grant) => {
                assert_eq!(grant.client_id.0.as_deref(), Some("mcp-client"));
            }
            _ => panic!("expected user"),
        }

        // Absent is the normal case on other paths and must not fail the token.
        let anonymous = issue_jwt(json!({
            "iss": "https://hydra.example.com",
            "sub": "alice",
            "aud": ["https://mcp.example.com"],
            "exp": now() + 300,
        }));
        match gate.authorize(Some(&anonymous), "").await {
            BearerAuthOutcome::Ok(grant) => assert_eq!(grant.client_id.0, None),
            _ => panic!("expected user"),
        }
    }

    #[test]
    fn challenge_carries_the_scope_only_when_there_is_one() {
        let gate = base_gate(|c| {
            c.resource_metadata_url = "https://sp.example.com/.well-known/x".to_string()
        });
        let stepped_up = gate.challenge_header(Some("insufficient_scope"), Some("stackpit:admin"));
        assert_eq!(
            stepped_up,
            "Bearer realm=\"test\", error=\"insufficient_scope\", scope=\"stackpit:admin\", \
             resource_metadata=\"https://sp.example.com/.well-known/x\"",
        );
        assert_eq!(
            gate.challenge_header(None, None),
            "Bearer realm=\"test\", resource_metadata=\"https://sp.example.com/.well-known/x\"",
        );
        assert!(!gate
            .challenge_header(Some("invalid_token"), Some(""))
            .contains("scope="));
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

    /// `jsonwebtoken` only compares `aud` when the claim is present, so an
    /// aud-less token from the right issuer would otherwise be accepted at a
    /// resource server that requires an audience (RFC 9068 §2.2, §4).
    #[tokio::test]
    async fn jwt_absent_audience_rejected() {
        let gate = base_gate(|_| {});
        let jwt = issue_jwt(json!({
            "iss": "https://hydra.example.com",
            "sub": "alice",
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

    // Signed with a different key than the one in the primed JWKS (same kid,
    // valid claims). Signature must fail against the primed public key.
    #[tokio::test]
    async fn jwt_wrong_signing_key_rejected() {
        const OTHER_PRIVATE_DER: &[u8] = include_bytes!("../testdata/test_rsa_priv_2.der");
        let gate = base_gate(|_| {});
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(TEST_KID.to_string());
        let key = EncodingKey::from_rsa_der(OTHER_PRIVATE_DER);
        let claims = json!({
            "iss": "https://hydra.example.com",
            "sub": "alice",
            "aud": ["https://mcp.example.com"],
            "exp": now() + 300,
            "iat": now(),
        });
        let jwt = encode(&header, &claims, &key).expect("sign with foreign key");
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

    // at capacity, LRU evicts oldest; touched entry survives
    #[tokio::test]
    async fn cache_lru_evicts_least_recently_used() {
        let gate = base_gate(|c| c.cache_ttl_secs = 10);
        let response = CachedResponse {
            sub: "alice".to_string(),
            sid: None,
            scope: None,
            client_id: None,
        };
        let now = now_secs();
        for i in 0..CACHE_CAPACITY {
            let mut key = [0u8; 32];
            key[..8].copy_from_slice(&(i as u64).to_le_bytes());
            gate.cache_store(key, &response, "iss", Some(now + 3600), now);
        }
        assert_eq!(gate.inner.cache.lock().len(), CACHE_CAPACITY);

        // touch must be strictly later than every fill insert
        tokio::time::sleep(Duration::from_millis(10)).await;

        // touch entry 0 so it becomes most-recently-used
        let mut touched = [0u8; 32];
        touched[..8].copy_from_slice(&0u64.to_le_bytes());
        let hit = gate.cache_lookup(&touched);
        assert!(hit.is_some(), "entry 0 should still be present");

        // a non-touched entry must be evicted, never entry 0
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

    // cache_store caps TTL at the configured ceiling
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
            client_id: None,
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
            client_id: None,
        };
        let now = now_secs();
        gate.cache_store(key, &response, "iss", Some(now + 3600), now);
        assert!(
            gate.cache_lookup(&key).is_none(),
            "ceiling=0 must short-circuit cache_store"
        );
    }

    // revocation cache hits within TTL skip the backing store
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

    fn spawn_introspection_server(
        body: String,
    ) -> (std::net::SocketAddr, Arc<std::sync::atomic::AtomicUsize>) {
        use std::sync::atomic::AtomicUsize;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_srv = hits.clone();
        let body: Arc<str> = body.into();

        tokio::spawn(async move {
            let listener = TcpListener::from_std(listener).unwrap();
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let hits = hits_srv.clone();
                let body = body.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = sock.read(&mut buf).await;
                    hits.fetch_add(1, Ordering::SeqCst);
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
        (addr, hits)
    }

    // inactive introspection result is negative-cached: repeat within TTL
    // skips the POST; force-expired entry re-introspects
    #[tokio::test]
    async fn opaque_inactive_introspection_negative_cached() {
        let (addr, hits) = spawn_introspection_server(r#"{"active":false}"#.to_string());
        let gate = base_gate(|c| c.introspection_url = Some(format!("http://{addr}/introspect")));
        let token = "opaque-forged-token";

        for _ in 0..3 {
            let outcome = gate.authorize(Some(token), "").await;
            assert!(matches!(outcome, BearerAuthOutcome::InvalidToken));
        }
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "repeat rejections within TTL must hit the negative cache"
        );

        // Force-expire the entry; the next call must introspect again.
        let key = cache::hash_token(token);
        gate.inner.negative_cache.lock().put(key, Instant::now());
        let outcome = gate.authorize(Some(token), "").await;
        assert!(matches!(outcome, BearerAuthOutcome::InvalidToken));
        assert_eq!(
            hits.load(Ordering::SeqCst),
            2,
            "expired negative entry must re-introspect"
        );
    }

    // valid opaque tokens are untouched by the negative cache
    #[tokio::test]
    async fn opaque_valid_token_not_negative_cached() {
        let body = format!(
            r#"{{"active":true,"sub":"alice","aud":["https://mcp.example.com"],"iss":"https://hydra.example.com","scope":"stackpit:events:read","exp":{}}}"#,
            now() + 300
        );
        let (addr, hits) = spawn_introspection_server(body);
        let gate = base_gate(|c| c.introspection_url = Some(format!("http://{addr}/introspect")));
        let token = "opaque-valid-token";

        for _ in 0..2 {
            match gate.authorize(Some(token), "stackpit:events:read").await {
                BearerAuthOutcome::Ok(BearerGrant {
                    ctx: AuthContext::User { sub, .. },
                    ..
                }) => assert_eq!(sub, "alice"),
                _ => panic!("expected user"),
            }
        }
        assert_eq!(
            hits.load(Ordering::SeqCst),
            2,
            "cache_ttl=0 gate must introspect both times (negative cache must not block)"
        );
        assert_eq!(
            gate.inner.negative_cache.lock().len(),
            0,
            "valid token must never land in the negative cache"
        );
    }

    // The `client_id` fallback exists for Hydra responses that omit `aud`. It
    // must not rescue a token that names a *different* resource: that would
    // turn every web-session token into a valid /mcp credential.
    #[tokio::test]
    async fn opaque_client_id_does_not_override_a_foreign_audience() {
        let body = format!(
            r#"{{"active":true,"sub":"alice","aud":["https://web.example.com"],"client_id":"stackpit-web","iss":"https://hydra.example.com","scope":"openid","exp":{}}}"#,
            now() + 300
        );
        let (addr, _) = spawn_introspection_server(body);
        let gate = base_gate(|c| {
            c.introspection_url = Some(format!("http://{addr}/introspect"));
            c.client_id = "stackpit-web".to_string();
        });
        let outcome = gate.authorize(Some("opaque-web-session-token"), "").await;
        assert!(matches!(outcome, BearerAuthOutcome::InvalidToken));
    }

    #[tokio::test]
    async fn opaque_client_id_still_rescues_an_absent_audience() {
        let body = format!(
            r#"{{"active":true,"sub":"alice","client_id":"stackpit-mcp","iss":"https://hydra.example.com","scope":"openid","exp":{}}}"#,
            now() + 300
        );
        let (addr, _) = spawn_introspection_server(body);
        let gate = base_gate(|c| c.introspection_url = Some(format!("http://{addr}/introspect")));
        let outcome = gate.authorize(Some("opaque-no-aud-token"), "").await;
        assert!(matches!(outcome, BearerAuthOutcome::Ok(_)));
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
            client_id: None,
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
