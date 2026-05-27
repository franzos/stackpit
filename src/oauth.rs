//! OIDC client: auth-code + PKCE flow (browser UI only; MCP uses introspection).
//! Tokens discarded after id_token verification; session cookie owns auth.
//! JWKS rotation: the shared [`stackpit_auth::JwksCache`] refetches on `kid` miss.

use std::sync::Arc;

use anyhow::{Context, Result};
use openidconnect::core::{
    CoreAuthenticationFlow, CoreClient, CoreIdTokenClaims, CoreIdTokenVerifier, CoreJsonWebKey,
    CoreProviderMetadata,
};
use openidconnect::{AuthorizationCode, JsonWebKeySet, OAuth2TokenResponse, TokenResponse};
use openidconnect::{
    ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope,
};
use secrecy::{ExposeSecret, SecretString};
use stackpit_auth::JwksCache;

use crate::config::OAuthConfig;

type CoreClientType = CoreClient<
    openidconnect::EndpointSet,
    openidconnect::EndpointNotSet,
    openidconnect::EndpointNotSet,
    openidconnect::EndpointNotSet,
    openidconnect::EndpointMaybeSet,
    openidconnect::EndpointMaybeSet,
>;

/// OIDC client (cheap to clone; Arc-shared inner state).
#[derive(Clone)]
pub struct OidcClient {
    inner: Arc<Inner>,
}

struct Inner {
    issuer: String,
    client_id: String,
    client_secret: SecretString,
    http: openidconnect::reqwest::Client,
    /// Immutable: JWKS rotation goes through the shared cache, not here.
    client: CoreClientType,
    jwks_uri: String,
    /// One cache per issuer, shared with MCP bearer + back-channel logout.
    jwks_cache: JwksCache,
    /// OIDC RP-Initiated Logout 1.0 §2.1. Optional -- not every IdP advertises it.
    end_session_endpoint: Option<String>,
    /// RFC 7662 discovery field. MCP gate falls back to this when
    /// `auth.mcp.introspection_url` is unset.
    introspection_endpoint: Option<String>,
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
}

impl OidcClient {
    /// OIDC Discovery 1.0 §4 + client build. Network call -- run at startup
    /// so the first login doesn't pay the round-trip. The [`JwksCache`] is
    /// shared with the MCP gate and back-channel logout handler.
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

        // SSRF defense + 10s cap so a hung IdP can't wedge the auth gate.
        let http = openidconnect::reqwest::Client::builder()
            .redirect(openidconnect::reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .context("building OAuth HTTP client")?;

        let client = build_client(issuer, client_id, client_secret, redirect_uri, &http).await?;

        // openidconnect's typed metadata doesn't surface end_session_endpoint
        // or introspection_endpoint; re-fetch the raw doc for those + jwks_uri.
        let endpoints = fetch_endpoint_urls(issuer, &http)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "failed to read discovery doc endpoints ({e:#}); RP-initiated logout, \
                 back-channel logout, and MCP opaque-token fallback may be unavailable"
                );
                DiscoveryEndpoints {
                    jwks_uri: format!("{}/.well-known/jwks.json", issuer.trim_end_matches('/')),
                    end_session_endpoint: None,
                    introspection_endpoint: None,
                }
            });

        // OIDC RP-Initiated Logout 1.0 §3 precondition. Pure decision in
        // `check_end_session_precondition` so it's unit-testable.
        match check_end_session_precondition(
            endpoints.end_session_endpoint.is_some(),
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

        let jwks_cache = JwksCache::new(
            http.clone(),
            endpoints.jwks_uri.clone(),
            jwks_cache_ttl_secs,
        );

        // Prime the cache so first-request JWT validation skips the RTT and
        // a flaky IdP can't 401 everything during recovery.
        match jwks_cache.prime().await {
            Ok(()) => tracing::info!(url = %endpoints.jwks_uri, "JWKS cache warmed at startup"),
            Err(e) if cfg.required => {
                return Err(anyhow::anyhow!(e)
                    .context("JWKS prime failed at startup and auth.oauth.required = true"));
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    url = %endpoints.jwks_uri,
                    "JWKS prime failed at startup; falling back to lazy kid-miss refetch",
                );
            }
        }

        Ok(Self {
            inner: Arc::new(Inner {
                issuer: issuer.to_string(),
                client_id: client_id.to_string(),
                client_secret: SecretString::from(client_secret.to_string()),
                http,
                client,
                jwks_uri: endpoints.jwks_uri,
                jwks_cache,
                end_session_endpoint: endpoints.end_session_endpoint,
                introspection_endpoint: endpoints.introspection_endpoint,
            }),
        })
    }

    pub fn issuer(&self) -> &str {
        &self.inner.issuer
    }

    pub fn client_id(&self) -> &str {
        &self.inner.client_id
    }

    pub fn jwks_uri(&self) -> &str {
        &self.inner.jwks_uri
    }

    pub fn end_session_endpoint(&self) -> Option<&str> {
        self.inner.end_session_endpoint.as_deref()
    }

    /// Discovery's `introspection_endpoint` (RFC 7662 extension). MCP gate
    /// falls back to this when `auth.mcp.introspection_url` is unset.
    pub fn introspection_endpoint(&self) -> Option<&str> {
        self.inner.introspection_endpoint.as_deref()
    }

    pub fn jwks_cache(&self) -> &JwksCache {
        &self.inner.jwks_cache
    }

    /// The shared HTTP client (redirects disabled, 10s timeout). Threaded
    /// into the bearer gates and any extra JWKS cache so the whole OIDC
    /// surface reuses one connection pool.
    pub fn http_client(&self) -> reqwest::Client {
        self.inner.http.clone()
    }

    /// Build authorize URL + PKCE/state/nonce for session.
    pub async fn start_login(&self) -> LoginStart {
        let client = &self.inner.client;
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let (auth_url, state, nonce) = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .add_scope(Scope::new("openid".to_string()))
            .add_scope(Scope::new("email".to_string()))
            .add_scope(Scope::new("profile".to_string()))
            .add_scope(Scope::new("offline_access".to_string()))
            .set_pkce_challenge(pkce_challenge)
            .url();

        LoginStart {
            auth_url: auth_url.to_string(),
            state: state.secret().to_string(),
            nonce: nonce.secret().to_string(),
            pkce_verifier: pkce_verifier.secret().to_string(),
        }
    }

    /// Exchange the code, verify the id_token, return claims + live tokens.
    /// Tokens are stored server-side; the browser only sees an opaque handle.
    pub async fn finish_login(
        &self,
        code: String,
        pkce_verifier: String,
        expected_nonce: &str,
    ) -> Result<LoginSuccess> {
        let client = &self.inner.client;
        let token_response = client
            .exchange_code(AuthorizationCode::new(code))
            .context("building code exchange request")?
            .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
            .request_async(&self.inner.http)
            .await
            .context("code exchange failed at the token endpoint")?;

        let id_token = token_response
            .id_token()
            .context("token endpoint did not return an id_token")?;
        let id_token_str = id_token.to_string();

        let nonce = Nonce::new(expected_nonce.to_string());

        let verifier = self.build_id_token_verifier(&id_token_str).await?;
        let claims = id_token
            .claims(&verifier, &nonce)
            .context("id_token verification failed")?;
        let mut login_claims = extract_login_claims(claims);

        // `sid` isn't in openidconnect 4's standard claim set; pull it from
        // the (already-verified) payload directly.
        login_claims.sid = extract_sid(&id_token_str);

        let access_token = token_response.access_token().secret().to_string();
        let access_exp = compute_access_exp(token_response.expires_in())?;
        let refresh_token = token_response
            .refresh_token()
            .map(|t| t.secret().to_string());
        // Hydra omits refresh-token expiry; cleanup falls back to the
        // configured ceiling.
        let refresh_exp = None;

        Ok(LoginSuccess {
            claims: login_claims,
            access_token,
            access_exp,
            refresh_token,
            refresh_exp,
            id_token: id_token_str,
        })
    }

    /// Build an [`IdTokenVerifier`](openidconnect::IdTokenVerifier) seeded
    /// from the shared JWKS cache. The unverified `kid` peek drives the
    /// refetch-on-miss path; the verifier then re-checks signed `iss`/`aud`.
    async fn build_id_token_verifier(&self, id_token_jwt: &str) -> Result<CoreIdTokenVerifier<'_>> {
        let header =
            jsonwebtoken::decode_header(id_token_jwt).context("id_token header undecodable")?;
        let kid = header
            .kid
            .ok_or_else(|| anyhow::anyhow!("id_token header missing kid"))?;
        let raw = self
            .inner
            .jwks_cache
            .raw_for_kid(&kid)
            .await
            .ok_or_else(|| anyhow::anyhow!("no JWK matches id_token kid {kid}"))?;

        let keys: JsonWebKeySet<CoreJsonWebKey> =
            serde_json::from_str(&raw).context("parsing JWKS for id_token verifier")?;

        let issuer = IssuerUrl::new(self.inner.issuer.clone())
            .with_context(|| format!("invalid issuer_url '{}'", self.inner.issuer))?;
        Ok(CoreIdTokenVerifier::new_confidential_client(
            ClientId::new(self.inner.client_id.clone()),
            ClientSecret::new(self.inner.client_secret.expose_secret().to_string()),
            issuer,
            keys,
        ))
    }

    /// Exchange a refresh token. Hydra rotates by default (OAuth 2.1 §4.3.2);
    /// callers MUST overwrite the stored value when the response carries a new one.
    pub async fn refresh(&self, refresh_token: &str) -> Result<RefreshSuccess, RefreshError> {
        use openidconnect::core::CoreErrorResponseType;
        use openidconnect::{RefreshToken, RequestTokenError};
        let client = &self.inner.client;
        let resp = match client
            .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
            .map_err(|e| RefreshError::Transient(format!("building refresh request: {e}")))?
            .request_async(&self.inner.http)
            .await
        {
            Ok(r) => r,
            Err(err) => {
                // OAuth 2.0 §5.2 `invalid_grant` = used/revoked/expired
                // refresh; force re-login. Everything else is transient.
                return Err(match err {
                    RequestTokenError::ServerResponse(ref server_err)
                        if matches!(server_err.error(), CoreErrorResponseType::InvalidGrant) =>
                    {
                        RefreshError::InvalidGrant
                    }
                    other => RefreshError::Transient(other.to_string()),
                });
            }
        };

        let access_token = resp.access_token().secret().to_string();
        let access_exp = compute_access_exp(resp.expires_in()).map_err(|e| {
            RefreshError::Transient(format!(
                "refresh response carried invalid expires_in: {e:#}"
            ))
        })?;
        let new_refresh = resp.refresh_token().map(|t| t.secret().to_string());

        Ok(RefreshSuccess {
            access_token,
            access_exp,
            // OAuth 2.0 §6 leaves rotation to the implementation; missing
            // refresh_token in the response means keep the existing one.
            refresh_token: new_refresh,
            refresh_exp: None,
        })
    }
}

/// Outcome of a successful refresh-token exchange.
pub struct RefreshSuccess {
    pub access_token: String,
    pub access_exp: i64,
    pub refresh_token: Option<String>,
    pub refresh_exp: Option<i64>,
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

/// Access-token expiry from `expires_in` (RFC 6749 §5.1). Missing or
/// overflowing = hard error: inventing a lifetime would let the refresh-
/// margin check skip refreshes for tokens already expired at the IdP.
fn compute_access_exp(expires_in: Option<std::time::Duration>) -> Result<i64> {
    let dur = expires_in.context(
        "OAuth token response omitted `expires_in`; cannot determine access-token lifetime. \
         Configure the IdP to emit `expires_in` (RFC 6749 §5.1)",
    )?;
    let secs =
        i64::try_from(dur.as_secs()).context("access-token `expires_in` overflows i64 seconds")?;
    Ok(chrono::Utc::now().timestamp() + secs)
}

fn extract_sid(id_token_jwt: &str) -> Option<String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    let payload_b64 = id_token_jwt.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    json.get("sid").and_then(|v| v.as_str()).map(String::from)
}

struct DiscoveryEndpoints {
    jwks_uri: String,
    end_session_endpoint: Option<String>,
    introspection_endpoint: Option<String>,
}

/// Pluck endpoints openidconnect's typed metadata doesn't surface.
async fn fetch_endpoint_urls(
    issuer: &str,
    http: &openidconnect::reqwest::Client,
) -> Result<DiscoveryEndpoints> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );
    let resp = http
        .get(&url)
        .send()
        .await
        .with_context(|| format!("fetching discovery doc at {url}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("discovery doc returned {}", resp.status());
    }
    let body = resp.text().await.context("reading discovery doc body")?;
    let json: serde_json::Value =
        serde_json::from_str(&body).context("parsing discovery doc JSON")?;
    let jwks_uri = json
        .get("jwks_uri")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("discovery doc missing jwks_uri"))?;
    // Drop non-https URLs -- a misconfigured discovery doc could flow `javascript:` into `Redirect::to(...)`.
    let end_session_endpoint = json
        .get("end_session_endpoint")
        .and_then(|v| v.as_str())
        .and_then(|raw| match validate_discovery_url(raw) {
            Some(url) => Some(url),
            None => {
                tracing::warn!(
                    raw = %raw,
                    "discovery: end_session_endpoint is not an absolute https URL or contains userinfo -- dropping"
                );
                None
            }
        });
    let introspection_endpoint = json
        .get("introspection_endpoint")
        .and_then(|v| v.as_str())
        .and_then(|raw| match validate_discovery_url(raw) {
            Some(url) => Some(url),
            None => {
                tracing::warn!(
                    raw = %raw,
                    "discovery: introspection_endpoint is not an absolute https URL or contains userinfo -- dropping"
                );
                None
            }
        });
    Ok(DiscoveryEndpoints {
        jwks_uri,
        end_session_endpoint,
        introspection_endpoint,
    })
}

/// Accept only absolute https URLs with a non-empty host and no userinfo.
fn validate_discovery_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = url::Url::parse(trimmed).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    let host = parsed.host_str()?;
    if host.is_empty() {
        return None;
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    Some(trimmed.to_string())
}

async fn build_client(
    issuer: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    http: &openidconnect::reqwest::Client,
) -> Result<CoreClientType> {
    let issuer_url = IssuerUrl::new(issuer.to_string())
        .with_context(|| format!("invalid issuer_url '{issuer}'"))?;
    let metadata = CoreProviderMetadata::discover_async(issuer_url, http)
        .await
        .with_context(|| format!("OIDC discovery failed for '{issuer}'"))?;

    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(client_id.to_string()),
        Some(ClientSecret::new(client_secret.to_string())),
    )
    .set_redirect_uri(
        RedirectUrl::new(redirect_uri.to_string())
            .with_context(|| format!("invalid redirect_uri '{redirect_uri}'"))?,
    );
    Ok(client)
}

fn extract_login_claims(claims: &CoreIdTokenClaims) -> LoginClaims {
    let iss = claims.issuer().to_string();
    let sub = claims.subject().to_string();
    // Unverified emails are attacker-controlled -- never use them for
    // identity decisions or unique-indexed columns.
    let email = match (claims.email(), claims.email_verified()) {
        (Some(addr), Some(true)) => Some(addr.as_str().to_string()),
        _ => None,
    };
    let name = claims
        .name()
        .and_then(|n| n.get(None))
        .map(|n| n.as_str().to_string());
    LoginClaims {
        iss,
        sub,
        email,
        name,
        sid: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
