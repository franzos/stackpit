//! OIDC client: auth-code + PKCE flow (browser UI only; MCP uses introspection).
//! Protocol work runs through `oidc_relying_party`. Provider metadata lives in a
//! [`DiscoveryHandle`] that re-fetches on a TTL; its [`JwksCache`] follows
//! `jwks_uri` and refetches on a `kid` miss.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use oidc_relying_party::authorize::{build_authorization_request, login_scopes};
use oidc_relying_party::discovery::{
    ClientAuthMethod, Discovery, DiscoveryConfig, DiscoveryHandle, DiscoveryHandleConfig,
};
use oidc_relying_party::id_token::{verify_id_token, IdTokenPolicy};
use oidc_relying_party::jwks::{JwksCache, JwksCacheConfig};
use oidc_relying_party::revocation;
use oidc_relying_party::token::{exchange_code, exchange_refresh_token, RefreshOutcome};
use secrecy::{ExposeSecret, SecretString};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::config::OAuthConfig;

/// OIDC client (cheap to clone; Arc-shared inner state).
#[derive(Clone)]
pub struct OidcClient {
    inner: Arc<Inner>,
}

struct Inner {
    /// Metadata and the key cache, shared with MCP bearer + back-channel logout.
    discovery: DiscoveryHandle,
    client_secret: SecretString,
    http: reqwest::Client,
    redirect_uri: Url,
    /// Scopes sent on the authorize request; see [`login_scopes`].
    scopes: Vec<String>,
    /// Client id, secret and the extra `aud` values an ID token may carry.
    id_token_policy: IdTokenPolicy,
    /// Hydra's non-standard `audience` and Forseti's `organization_id`, when configured.
    extra_params: Vec<(String, String)>,
}

/// Auth start: auth URL + session secrets (state/nonce/PKCE).
pub struct LoginStart {
    pub auth_url: String,
    pub state: String,
    pub nonce: String,
    /// PKCE verifier -- never leaves the server, paired with the code on exchange.
    pub pkce_verifier: String,
}

/// Auth finish: verified claims (email only if email_verified=true).
pub struct LoginClaims {
    pub iss: String,
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    /// OIDC Session Management 1.0 §5. Dedupe key for back-channel logout.
    /// Provider-dependent -- Hydra emits it, not every IdP does.
    pub sid: Option<String>,
    /// None = orgs claim absent (scope not granted); distinct from Some(empty) = granted, no orgs.
    pub orgs: Option<Vec<OrgClaim>>,
    /// Mirrors the `orgs_truncated` flag from Forseti; true means the list was capped.
    pub orgs_truncated: bool,
    /// Validated BCP-47 tag of the OIDC `locale` claim (e.g. "de"). None if the
    /// claim is absent or not a SUPPORTED locale.
    pub locale: Option<String>,
}

/// Verified claims plus live IdP tokens. Stored server-side, encrypted; the
/// browser only sees the opaque handle.
pub struct LoginSuccess {
    pub claims: LoginClaims,
    pub access_token: String,
    pub access_exp: i64,
    pub refresh_token: Option<String>,
    /// `None` = unknown lifetime; cleanup falls back to a configured ceiling
    /// (Hydra omits this field).
    pub refresh_exp: Option<i64>,
    /// Required as `id_token_hint` on RP-initiated logout.
    pub id_token: String,
    /// The granted `scope` includes `offline_access`.
    pub offline: bool,
}

impl OidcClient {
    /// OIDC Discovery 1.0 §4 + JWKS prime. Network call -- run at startup
    /// so the first login doesn't pay the round-trip. The [`DiscoveryHandle`]
    /// is shared with the MCP gate and back-channel logout handler; call
    /// [`Self::spawn_refresh`] to keep it current.
    pub async fn discover(cfg: &OAuthConfig, jwks_cache_ttl_secs: u64) -> Result<Self> {
        let issuer = cfg
            .issuer_url
            .as_deref()
            .context("auth.oauth.issuer_url required")?;
        let client_id = cfg
            .client_id
            .as_deref()
            .context("auth.oauth.client_id required")?;
        let client_secret = cfg
            .client_secret
            .as_ref()
            .map(ExposeSecret::expose_secret)
            .context("auth.oauth.client_secret required")?;
        let redirect_uri = cfg
            .redirect_uri
            .as_deref()
            .context("auth.oauth.redirect_uri required")?;
        let redirect_uri = Url::parse(redirect_uri)
            .with_context(|| format!("invalid redirect_uri '{redirect_uri}'"))?;

        // SSRF defense + 10s cap so a hung IdP can't wedge the auth gate.
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .context("building OAuth HTTP client")?;

        let discovery_config = DiscoveryConfig {
            token_endpoint_auth_method: cfg
                .token_endpoint_auth_method
                .as_deref()
                .map(ClientAuthMethod::parse)
                .transpose()
                .context("auth.oauth.token_endpoint_auth_method")?,
        };
        let handle = DiscoveryHandle::connect(
            http.clone(),
            issuer,
            DiscoveryHandleConfig {
                discovery: discovery_config,
                jwks: jwks_cache_config(jwks_cache_ttl_secs),
                ..DiscoveryHandleConfig::default()
            },
        )
        .await
        .with_context(|| format!("OIDC discovery failed for '{issuer}'"))?;
        let discovery = handle.current();

        // OIDC RP-Initiated Logout 1.0 §3 precondition. Pure decision in
        // `check_end_session_precondition` so it's unit-testable.
        match check_end_session_precondition(
            discovery.end_session_endpoint.is_some(),
            cfg.required,
            cfg.allow_local_only_logout,
        ) {
            EndSessionDecision::Ok => {}
            EndSessionDecision::Warn => {
                tracing::warn!(
                    "OIDC discovery omits end_session_endpoint; RP-initiated logout will sign \
                     out of Stackpit only, not the IdP. Set auth.oauth.allow_local_only_logout \
                     = true to silence this warning."
                );
            }
            EndSessionDecision::Fail => {
                anyhow::bail!(
                    "OIDC discovery doc omits end_session_endpoint (required by OIDC RP-Initiated \
                     Logout 1.0 §3) and auth.oauth.required = true. RP-initiated logout cannot \
                     fire; the IdP session would survive Stackpit logout. Either configure your \
                     IdP to advertise end_session_endpoint, or set \
                     auth.oauth.allow_local_only_logout = true to accept local-only logout."
                );
            }
        }

        // Prime the cache so first-request JWT validation skips the RTT and
        // a flaky IdP can't 401 everything during recovery.
        match handle.jwks().prime().await {
            Ok(()) => tracing::info!(url = %discovery.jwks_uri, "JWKS cache warmed at startup"),
            Err(e) if cfg.required => {
                return Err(anyhow::anyhow!(e)
                    .context("JWKS prime failed at startup and auth.oauth.required = true"));
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    url = %discovery.jwks_uri,
                    "JWKS prime failed at startup; falling back to lazy kid-miss refetch",
                );
            }
        }

        let mut extra_params = Vec::new();
        if !cfg.web_audience.is_empty() {
            extra_params.push(("audience".to_string(), cfg.web_audience.clone()));
        }
        if let Some(org) = cfg.organization_id.as_deref().filter(|o| !o.is_empty()) {
            extra_params.push(("organization_id".to_string(), org.to_string()));
        }

        Ok(Self {
            inner: Arc::new(Inner {
                discovery: handle,
                client_secret: SecretString::from(client_secret.to_string()),
                http,
                redirect_uri,
                scopes: login_scopes(&cfg.scopes),
                id_token_policy: IdTokenPolicy {
                    client_id: client_id.to_string(),
                    client_secret: Some(client_secret.to_string()),
                    trusted_audiences: cfg.trusted_audiences.clone(),
                    max_iat_age: None,
                    max_age: None,
                },
                extra_params,
            }),
        })
    }

    pub fn issuer(&self) -> &str {
        self.inner.discovery.issuer()
    }

    pub fn client_id(&self) -> &str {
        &self.inner.id_token_policy.client_id
    }

    /// The current validated provider metadata. Take one snapshot per operation.
    pub fn discovery(&self) -> Arc<Discovery> {
        self.inner.discovery.current()
    }

    /// The handle the metadata and key cache are read through.
    pub fn discovery_handle(&self) -> DiscoveryHandle {
        self.inner.discovery.clone()
    }

    pub fn jwks_uri(&self) -> Url {
        self.discovery().jwks_uri.clone()
    }

    /// Discovery's `introspection_endpoint` (RFC 7662 extension). MCP gate
    /// falls back to this when `auth.mcp.introspection_url` is unset.
    pub fn introspection_endpoint(&self) -> Option<String> {
        self.discovery()
            .introspection_endpoint
            .as_ref()
            .map(|url| url.as_str().to_string())
    }

    /// Re-fetches discovery on the handle's TTL until `cancel` fires.
    pub fn spawn_refresh(&self, cancel: CancellationToken) {
        let handle = self.inner.discovery.clone();
        crate::background::supervise("oidc_discovery_refresh", async move {
            tokio::select! {
                _ = cancel.cancelled() => {}
                () = handle.run_refresh() => {}
            }
        });
    }

    /// Best-effort RFC 7009 revocation of a refresh token.
    ///
    /// Forgetting the grant row only stops *this* deployment honouring it; the
    /// token stays live at the IdP until it expires, and anything that lifted
    /// it (a backup, a log, a compromised DB) can still redeem it. Failure is
    /// logged and swallowed: logout must complete regardless.
    pub async fn revoke_refresh_token(&self, refresh_token: &str) {
        let discovery = self.discovery();
        if discovery.revocation_endpoint.is_none() {
            tracing::debug!("logout: IdP advertises no revocation_endpoint; skipping revoke");
            return;
        }
        match revocation::revoke_refresh_token(
            &self.inner.http,
            &discovery,
            self.client_id(),
            Some(self.inner.client_secret.expose_secret()),
            discovery.token_endpoint_auth_method,
            refresh_token,
        )
        .await
        {
            Ok(()) => tracing::debug!("logout: refresh token revoked at the IdP"),
            Err(e) => tracing::warn!(error = %e, "logout: refresh token revoke failed"),
        }
    }

    /// Read the `orgs` claim for the holder of `access_token` (OIDC Core 1.0
    /// §5.3). The MCP path calls this per request: a bearer caller never runs
    /// the browser callback, so this is the only place an IdP-side demotion can
    /// reach Stackpit.
    pub async fn fetch_userinfo_orgs(
        &self,
        access_token: &str,
    ) -> Result<OrgsClaim, UserinfoError> {
        let url = self
            .discovery()
            .userinfo_endpoint
            .clone()
            .ok_or(UserinfoError::NotConfigured)?;

        let resp = self
            .inner
            .http
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "userinfo request failed");
                UserinfoError::Unavailable
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(UserinfoError::TokenRejected);
        }
        if !status.is_success() {
            tracing::warn!(%status, "userinfo returned non-2xx");
            return Err(UserinfoError::Unavailable);
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| {
            tracing::warn!(error = %e, "userinfo returned unparseable JSON");
            UserinfoError::Unavailable
        })?;
        Ok(parse_orgs_claim(
            json.get("orgs"),
            json.get("orgs_truncated"),
        ))
    }

    /// The key cache, pointed at the current `jwks_uri`.
    pub fn jwks_cache(&self) -> JwksCache {
        self.inner.discovery.jwks()
    }

    /// The shared HTTP client (redirects disabled, 10s timeout). Threaded
    /// into the bearer gates and any extra JWKS cache so the whole OIDC
    /// surface reuses one connection pool.
    pub fn http_client(&self) -> reqwest::Client {
        self.inner.http.clone()
    }

    /// Build authorize URL + PKCE/state/nonce for session.
    pub fn start_login(&self) -> Result<LoginStart> {
        let discovery = self.discovery();
        let req = build_authorization_request(
            &discovery,
            self.client_id(),
            &self.inner.redirect_uri,
            &self.inner.scopes,
            &self.inner.extra_params,
        )
        .context("building the authorization request")?;
        Ok(LoginStart {
            auth_url: req.url.to_string(),
            state: req.state,
            nonce: req.nonce,
            pkce_verifier: req.pkce_verifier,
        })
    }

    /// Exchange the code, verify the id_token, return claims + live tokens.
    /// Tokens are stored server-side; the browser only sees an opaque handle.
    pub async fn finish_login(
        &self,
        code: String,
        pkce_verifier: String,
        expected_nonce: &str,
    ) -> Result<LoginSuccess> {
        let inner = &self.inner;
        let discovery = self.discovery();
        let jwks = self.jwks_cache();
        let tokens = exchange_code(
            &inner.http,
            &discovery,
            self.client_id(),
            Some(inner.client_secret.expose_secret()),
            &inner.redirect_uri,
            &code,
            &pkce_verifier,
        )
        .await
        .context("code exchange failed at the token endpoint")?;
        let id_token = tokens
            .id_token
            .clone()
            .expect("exchange_code returns an id_token");
        // RFC 6749 §5.1: an omitted `scope` means the requested scopes were granted.
        let granted = if tokens.scopes.is_empty() {
            &inner.scopes
        } else {
            &tokens.scopes
        };
        let offline = granted.iter().any(|s| s == "offline_access");

        let claims = verify_id_token(
            &jwks,
            &discovery,
            &id_token,
            Some(expected_nonce),
            Some(&tokens.access_token),
            &inner.id_token_policy,
        )
        .await
        .context("id_token verification failed")?;

        let orgs = parse_orgs_claim(
            claims.extra_claim("orgs"),
            claims.extra_claim("orgs_truncated"),
        );
        // Untrusted provider value: keep only if it negotiates to a SUPPORTED locale.
        let locale = claims
            .locale
            .as_deref()
            .and_then(crate::locale::accept)
            .map(|l| l.to_string());
        let login_claims = LoginClaims {
            iss: claims.issuer,
            sub: claims.subject,
            email: claims.email,
            name: claims.name,
            sid: claims.sid,
            orgs: orgs.orgs,
            orgs_truncated: orgs.truncated,
            locale,
        };

        Ok(LoginSuccess {
            claims: login_claims,
            access_exp: compute_access_exp(tokens.expires_in)?,
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            // Hydra omits refresh-token expiry; cleanup falls back to the
            // configured ceiling.
            refresh_exp: None,
            id_token,
            offline,
        })
    }

    /// Exchange a refresh token. Hydra rotates by default (OAuth 2.1 §4.3.2);
    /// callers MUST overwrite the stored value when the response carries a new one.
    /// A returned ID token must verify and name `expected_sub` (OIDC Core 12.2).
    pub async fn refresh(
        &self,
        refresh_token: &str,
        expected_sub: &str,
    ) -> Result<RefreshSuccess, RefreshError> {
        let inner = &self.inner;
        let discovery = self.discovery();
        let jwks = self.jwks_cache();
        let tokens = match exchange_refresh_token(
            &inner.http,
            &discovery,
            self.client_id(),
            Some(inner.client_secret.expose_secret()),
            refresh_token,
            &jwks,
            &inner.id_token_policy,
            expected_sub,
        )
        .await
        {
            RefreshOutcome::Refreshed(tokens) => tokens,
            RefreshOutcome::InvalidGrant => return Err(RefreshError::InvalidGrant),
            RefreshOutcome::IdTokenRejected(e) => {
                tracing::warn!(error = %e, "refreshed id_token rejected; ending the grant");
                return Err(RefreshError::InvalidGrant);
            }
            RefreshOutcome::Transient(e) => return Err(RefreshError::Transient(e.to_string())),
            _ => {
                return Err(RefreshError::Transient(
                    "unrecognised refresh outcome".to_string(),
                ))
            }
        };

        let access_exp = compute_access_exp(tokens.expires_in).map_err(|e| {
            RefreshError::Transient(format!(
                "refresh response carried invalid expires_in: {e:#}"
            ))
        })?;

        // RFC 6749 §5.1: `scope` may be omitted when unchanged.
        let offline = (!tokens.scopes.is_empty())
            .then(|| tokens.scopes.iter().any(|s| s == "offline_access"));
        Ok(RefreshSuccess {
            access_token: tokens.access_token,
            access_exp,
            // OAuth 2.0 §6 leaves rotation to the implementation; missing
            // refresh_token in the response means keep the existing one.
            refresh_token: tokens.refresh_token,
            refresh_exp: None,
            id_token: tokens.id_token,
            offline,
        })
    }
}

/// JWKS cache tuning: the configured TTL, the crate's defaults otherwise.
pub fn jwks_cache_config(ttl_secs: u64) -> JwksCacheConfig {
    JwksCacheConfig {
        ttl: Duration::from_secs(ttl_secs),
        ..JwksCacheConfig::default()
    }
}

/// Outcome of a successful refresh-token exchange.
pub struct RefreshSuccess {
    pub access_token: String,
    pub access_exp: i64,
    pub refresh_token: Option<String>,
    pub refresh_exp: Option<i64>,
    /// A new ID token, verified and naming the grant's `sub`, when the IdP returned one.
    pub id_token: Option<String>,
    /// `offline_access` in the granted `scope`; `None` when the response omitted `scope`.
    pub offline: Option<bool>,
}

/// `InvalidGrant` = force re-login; `Transient` = try existing token, retry next.
#[derive(Debug)]
pub enum RefreshError {
    InvalidGrant,
    Transient(String),
}

impl std::fmt::Display for RefreshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefreshError::InvalidGrant => write!(f, "invalid_grant (re-login required)"),
            RefreshError::Transient(s) => write!(f, "transient refresh error: {s}"),
        }
    }
}

impl std::error::Error for RefreshError {}

/// `end_session_endpoint` precondition decision.
#[derive(Debug, PartialEq, Eq)]
enum EndSessionDecision {
    /// Endpoint present, or operator opted in.
    Ok,
    /// Missing on `required = false` -- warn at startup, continue.
    Warn,
    /// Missing on `required = true` without opt-in -- refuse to start.
    Fail,
}

fn check_end_session_precondition(
    has_endpoint: bool,
    required: bool,
    allow_local_only_logout: bool,
) -> EndSessionDecision {
    if has_endpoint || allow_local_only_logout {
        return EndSessionDecision::Ok;
    }
    if required {
        EndSessionDecision::Fail
    } else {
        EndSessionDecision::Warn
    }
}

/// Access-token expiry from `expires_in`. An overflowing value is an error.
fn compute_access_exp(expires_in: Duration) -> Result<i64> {
    let secs = i64::try_from(expires_in.as_secs())
        .context("access-token `expires_in` overflows i64 seconds")?;
    Ok(chrono::Utc::now().timestamp() + secs)
}

// Serialize/Deserialize required: Task 1.9 JSON-packs Vec<OrgClaim> into the signed sp_provision cookie.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrgClaim {
    pub id: String,
    pub slug: String,
    pub role: String,
    pub name: Option<String>,
}

/// The `orgs` claim as [`reconcile`](crate::orgs::reconcile::reconcile) wants
/// it. `orgs: None` = claim absent (zero authority, no removals); `Some(empty)`
/// = granted, no orgs.
#[derive(Debug, Default)]
pub struct OrgsClaim {
    pub orgs: Option<Vec<OrgClaim>>,
    pub truncated: bool,
}

/// Why a userinfo call failed. Callers fail closed on all three, but only
/// `TokenRejected` means the credential itself is dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserinfoError {
    TokenRejected,
    Unavailable,
    NotConfigured,
}

/// Shared by the id_token path (browser login) and the userinfo path (MCP).
/// Truncation comes from the flag only.
fn parse_orgs_claim(
    orgs: Option<&serde_json::Value>,
    truncated: Option<&serde_json::Value>,
) -> OrgsClaim {
    let truncated = truncated.and_then(|v| v.as_bool()).unwrap_or(false);
    let Some(arr) = orgs.and_then(|v| v.as_array()) else {
        return OrgsClaim {
            orgs: None,
            truncated,
        };
    };
    let orgs = arr
        .iter()
        .filter_map(|o| {
            Some(OrgClaim {
                id: o.get("id")?.as_str()?.to_string(),
                slug: o
                    .get("slug")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                role: o
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("member")
                    .to_string(),
                name: o.get("name").and_then(|v| v.as_str()).map(String::from),
            })
        })
        .collect::<Vec<_>>();
    OrgsClaim {
        orgs: Some(orgs),
        truncated,
    }
}

/// In-memory metadata for `issuer`: every accepted algorithm, no optional endpoints.
#[cfg(test)]
pub(crate) fn test_discovery(issuer: &str) -> Discovery {
    let url = |path: &str| Url::parse(&format!("{issuer}{path}")).expect("stub URL valid");
    Discovery {
        issuer: issuer.to_string(),
        authorization_endpoint: url("/oauth2/auth"),
        token_endpoint: url("/oauth2/token"),
        jwks_uri: url("/.well-known/jwks.json"),
        userinfo_endpoint: None,
        end_session_endpoint: None,
        revocation_endpoint: None,
        introspection_endpoint: None,
        id_token_signing_alg_values_supported: Vec::new(),
        signing_algorithms: oidc_relying_party::algorithms::ASYMMETRIC_ALGORITHMS.to_vec(),
        token_endpoint_auth_method: ClientAuthMethod::Basic,
        authorization_response_iss_parameter_supported: false,
    }
}

impl OidcClient {
    /// Test-only constructor over a fixed handle (no network).
    #[cfg(test)]
    pub(crate) fn for_test(client_id: &str, discovery: DiscoveryHandle) -> Self {
        let issuer = discovery.issuer().to_string();
        Self {
            inner: Arc::new(Inner {
                discovery,
                client_secret: SecretString::from("test-secret".to_string()),
                http: reqwest::Client::new(),
                redirect_uri: Url::parse(&format!("{issuer}/callback")).expect("stub URL valid"),
                scopes: login_scopes(&[]),
                id_token_policy: IdTokenPolicy {
                    client_id: client_id.to_string(),
                    client_secret: Some("test-secret".to_string()),
                    trusted_audiences: Vec::new(),
                    max_iat_age: None,
                    max_age: None,
                },
                extra_params: Vec::new(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oidc_relying_party::algorithms::SigningAlgorithm;
    use oidc_relying_party::test_support::{
        jwks, rsa_key, sign_id_token, unix_now, FakeIdp, TOKEN_PATH,
    };
    use serde_json::json;

    const ISSUER: &str = "https://idp.example.com";

    fn test_cache() -> JwksCache {
        JwksCache::new(
            reqwest::Client::new(),
            Url::parse("https://idp.example.com/.well-known/jwks.json").unwrap(),
            jwks_cache_config(60),
        )
    }

    fn test_client() -> OidcClient {
        OidcClient::for_test(
            "stackpit",
            DiscoveryHandle::fixed(test_discovery(ISSUER), test_cache()),
        )
    }

    fn oauth_config(issuer: &str) -> OAuthConfig {
        OAuthConfig {
            issuer_url: Some(issuer.to_string()),
            client_id: Some("stackpit".to_string()),
            client_secret: Some(SecretString::from("s")),
            redirect_uri: Some("https://sp/web/auth/callback".to_string()),
            allow_local_only_logout: true,
            ..OAuthConfig::default()
        }
    }

    #[tokio::test]
    async fn refreshed_discovery_is_visible_through_the_client() {
        let mut idp =
            FakeIdp::start(&[SigningAlgorithm::Rs256], &jwks(&[rsa_key()]), json!({})).await;
        let client = OidcClient::discover(&oauth_config(&idp.issuer()), 60)
            .await
            .unwrap();
        assert_eq!(
            client.discovery().signing_algorithms,
            vec![SigningAlgorithm::Rs256]
        );
        assert_eq!(client.jwks_uri().as_str(), idp.jwks_url());
        assert!(client.introspection_endpoint().is_none());

        let moved = format!("{}/keys", idp.url());
        let introspect = format!("{}/oauth2/introspect", idp.url());
        idp.set_discovery(json!({
            "jwks_uri": moved,
            "introspection_endpoint": introspect,
            "id_token_signing_alg_values_supported": ["RS256", "ES256"],
        }))
        .await;
        client.discovery_handle().refresh().await.unwrap();

        let after = client.discovery();
        assert_eq!(
            after.signing_algorithms,
            vec![SigningAlgorithm::Rs256, SigningAlgorithm::Es256]
        );
        assert_eq!(after.jwks_uri.as_str(), moved);
        assert_eq!(client.jwks_uri().as_str(), moved);
        assert_eq!(
            client.introspection_endpoint().as_deref(),
            Some(introspect.as_str())
        );
    }

    /// Runs the code exchange against a token response without `scope`.
    async fn finish_login_without_scope_in_response(requested: &[&str]) -> LoginSuccess {
        let key = rsa_key();
        let mut idp = FakeIdp::start(
            &[SigningAlgorithm::Rs256],
            &jwks(std::slice::from_ref(&key)),
            json!({}),
        )
        .await;
        let issuer = idp.issuer();
        let now = unix_now();
        let id_token = sign_id_token(
            &key,
            Some(key.kid()),
            &json!({
                "iss": issuer,
                "sub": "alice",
                "aud": "stackpit",
                "iat": now,
                "exp": now + 300,
                "nonce": "n-1",
            }),
        );
        idp.server()
            .mock("POST", TOKEN_PATH)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "access_token": "at",
                    "token_type": "Bearer",
                    "expires_in": 300,
                    "id_token": id_token,
                })
                .to_string(),
            )
            .create_async()
            .await;

        let mut cfg = oauth_config(&issuer);
        cfg.scopes = requested.iter().map(ToString::to_string).collect();
        let client = OidcClient::discover(&cfg, 60).await.unwrap();
        client
            .finish_login("code".to_string(), "verifier".to_string(), "n-1")
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn finish_login_offline_falls_back_to_the_requested_scopes() {
        let granted = finish_login_without_scope_in_response(&["openid", "offline_access"]).await;
        assert!(granted.offline);

        let plain = finish_login_without_scope_in_response(&[]).await;
        assert!(!plain.offline);
    }

    #[test]
    fn extra_params_only_when_configured() {
        let mut client = test_client();
        let params = |client: &OidcClient| -> std::collections::HashMap<String, String> {
            let url = Url::parse(&client.start_login().unwrap().auth_url).unwrap();
            url.query_pairs().into_owned().collect()
        };
        let plain = params(&client);
        assert!(!plain.contains_key("audience"));
        assert!(!plain.contains_key("organization_id"));
        assert_eq!(plain["scope"], "openid email profile");

        Arc::get_mut(&mut client.inner).unwrap().extra_params = vec![
            ("audience".to_string(), "stackpit-web".to_string()),
            ("organization_id".to_string(), "acme".to_string()),
        ];
        let scoped = params(&client);
        assert_eq!(scoped["audience"], "stackpit-web");
        assert_eq!(scoped["organization_id"], "acme");
    }

    #[test]
    fn orgs_claim_reads_array_and_truncation() {
        let claims = json!({
            "orgs": [
                {"id": "acme", "slug": "acme", "role": "owner", "name": "Acme"},
                {"id": "default", "slug": "default", "role": "member", "name": "Default"}
            ],
            "orgs_truncated": true
        });
        let parsed = parse_orgs_claim(claims.get("orgs"), claims.get("orgs_truncated"));
        let orgs = parsed.orgs.unwrap();
        assert_eq!(orgs.len(), 2);
        assert_eq!(orgs[0].id, "acme");
        assert_eq!(orgs[0].role, "owner");
        assert!(parsed.truncated);
    }

    #[test]
    fn orgs_claim_absent_is_none() {
        let parsed = parse_orgs_claim(None, None);
        assert!(parsed.orgs.is_none());
        assert!(!parsed.truncated);
    }

    #[test]
    fn end_session_present_is_ok_regardless_of_flags() {
        assert_eq!(
            check_end_session_precondition(true, true, false),
            EndSessionDecision::Ok
        );
        assert_eq!(
            check_end_session_precondition(true, false, false),
            EndSessionDecision::Ok
        );
        assert_eq!(
            check_end_session_precondition(true, true, true),
            EndSessionDecision::Ok
        );
    }

    #[test]
    fn end_session_missing_required_without_optin_fails() {
        assert_eq!(
            check_end_session_precondition(false, true, false),
            EndSessionDecision::Fail
        );
    }

    #[test]
    fn end_session_missing_required_with_optin_ok() {
        assert_eq!(
            check_end_session_precondition(false, true, true),
            EndSessionDecision::Ok
        );
    }

    #[test]
    fn end_session_missing_optional_without_optin_warns() {
        assert_eq!(
            check_end_session_precondition(false, false, false),
            EndSessionDecision::Warn
        );
    }

    #[test]
    fn end_session_missing_optional_with_optin_ok() {
        assert_eq!(
            check_end_session_precondition(false, false, true),
            EndSessionDecision::Ok
        );
    }
}
