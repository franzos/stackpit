use anyhow::Result;
use secrecy::ExposeSecret;
use std::path::Path;

use super::url::{url_origin, validate_absolute_http_url, validate_issuer_url_scheme};
use super::Config;

/// Hard cap on `refresh_token_max_ttl_secs`. Larger values would let
/// revocation markers accumulate for months.
const REFRESH_TOKEN_MAX_TTL_CAP_SECS: u64 = 90 * 24 * 3600;

/// Cap for `cache_max_ttl_secs` (oauth + mcp). Higher values let scope/audience
/// rotations linger past the next IdP rotation.
const BEARER_CACHE_MAX_TTL_CAP_SECS: u64 = 300;

impl Config {
    /// Catches misconfigurations early -- call right after loading.
    pub fn validate(&self) -> Result<()> {
        use std::net::SocketAddr;

        // Make sure bind addresses are parseable
        self.server
            .bind
            .parse::<SocketAddr>()
            .map_err(|e| anyhow::anyhow!("invalid server.bind '{}': {e}", self.server.bind))?;

        self.server.ingest_bind.parse::<SocketAddr>().map_err(|e| {
            anyhow::anyhow!(
                "invalid server.ingest_bind '{}': {e}",
                self.server.ingest_bind
            )
        })?;

        // Sanity-check the external URL scheme
        if let Some(ref url) = self.server.external_url {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                anyhow::bail!(
                    "server.external_url must start with http:// or https://, got '{url}'"
                );
            }
        }
        if let Some(ref url) = self.server.external_ingest_url {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                anyhow::bail!(
                    "server.external_ingest_url must start with http:// or https://, got '{url}'"
                );
            }
        }

        // OIDC redirect URIs: misconfig would become an open redirect at
        // logout. Reject anything that isn't an absolute http(s) URL.
        validate_absolute_http_url(
            "auth.oauth.redirect_uri",
            self.auth.oauth.redirect_uri.as_deref(),
        )?;
        validate_absolute_http_url(
            "auth.oauth.post_logout_redirect_uri",
            self.auth.oauth.post_logout_redirect_uri.as_deref(),
        )?;

        // Cross-origin post-logout target: a typo'd host bounces the user
        // somewhere unexpected via Hydra's redirect. Require explicit opt-in.
        if let (Some(post), Some(ext)) = (
            self.auth.oauth.post_logout_redirect_uri.as_deref(),
            self.server.external_url.as_deref(),
        ) {
            if let (Some(post_host), Some(ext_host)) = (url_origin(post), url_origin(ext)) {
                if post_host != ext_host {
                    if !self.auth.oauth.post_logout_allow_cross_origin {
                        anyhow::bail!(
                            "auth.oauth.post_logout_redirect_uri origin '{post_host}' differs \
                             from server.external_url origin '{ext_host}'. Set \
                             auth.oauth.post_logout_allow_cross_origin = true to opt in."
                        );
                    }
                    tracing::warn!(
                        "auth.oauth.post_logout_redirect_uri origin '{post_host}' differs from \
                         server.external_url origin '{ext_host}' (allowed via \
                         post_logout_allow_cross_origin)"
                    );
                }
            }
        }

        // Without audience or scope binding, an IdP-issued token for another
        // resource server would pass the admin UI gate. `allow_empty_web_audience`
        // is the deprecated opt-out.
        if self.auth.oauth.required
            && self.auth.oauth.web_audience.trim().is_empty()
            && self.auth.oauth.web_required_scope.trim().is_empty()
            && !self.auth.oauth.allow_empty_web_audience
        {
            anyhow::bail!(
                "auth.oauth.required = true but both auth.oauth.web_audience and \
                 auth.oauth.web_required_scope are empty. Set at least one so the \
                 web bearer gate binds tokens to this resource server, or set \
                 auth.oauth.allow_empty_web_audience = true to opt in to the legacy \
                 behaviour during deprecation."
            );
        }

        // Create the DB directory if it doesn't exist yet
        let db_path = Path::new(&self.storage.path);
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    anyhow::anyhow!(
                        "cannot create database directory '{}': {e}",
                        parent.display()
                    )
                })?;
            }
        }

        // Catch empty or too-short admin tokens early
        if let Some(ref token) = self.server.admin_token {
            let trimmed = token.expose_secret().trim();
            if trimmed.is_empty() {
                anyhow::bail!(
                    "server.admin_token is set but empty — either remove it or set a real token"
                );
            }
            if trimmed.len() < 16 {
                anyhow::bail!(
                    "server.admin_token is too short ({} chars) — use at least 16 characters",
                    trimmed.len()
                );
            }
        }

        // Non-loopback bind + auth without Secure cookies = session cookies
        // sniffable over plain HTTP.
        let admin_loopback = self
            .server
            .bind
            .parse::<SocketAddr>()
            .map(|addr| addr.ip().is_loopback())
            .unwrap_or(false);
        let auth_enabled = self.server.admin_token.is_some() || self.auth.oauth.is_enabled();
        if auth_enabled && !admin_loopback && !self.server.force_secure_cookies {
            anyhow::bail!(
                "server.bind '{}' is not loopback and auth is enabled, but \
                 server.force_secure_cookies = false. Hostile-internet deployments \
                 MUST set force_secure_cookies = true so session cookies are flagged \
                 Secure and not sniffable over plain HTTP. Bind to 127.0.0.1 only if \
                 you really intend plain HTTP.",
                self.server.bind
            );
        }

        // Unprotected admin UI: only allowed on loopback with explicit
        // `no_auth_loopback_acknowledged = true`. OAuth-only is also fine.
        if self.server.admin_token.is_none() && !self.auth.oauth.is_enabled() {
            let loopback = self
                .server
                .bind
                .parse::<SocketAddr>()
                .map(|addr| addr.ip().is_loopback())
                .unwrap_or(false);
            if !loopback || !self.server.no_auth_loopback_acknowledged {
                anyhow::bail!(
                    "no auth mode configured; refusing to start. Set either `server.admin_token`, \
                     enable `[auth.oauth]`, or bind to loopback and acknowledge with \
                     `server.no_auth_loopback_acknowledged = true`."
                );
            }
            tracing::warn!(
                "no auth mode active; loopback-only deployment acknowledged via \
                 server.no_auth_loopback_acknowledged"
            );
        }

        // A locked mailer with no sender or no token can never send.
        if self.email.lock {
            if self.email.from_address.is_none() {
                anyhow::bail!(
                    "email.lock = true but email.from_address is unset. A locked mailer needs \
                     a sender; set email.from_address or unset email.lock."
                );
            }
            if self.email.token.is_none() {
                anyhow::bail!(
                    "email.lock = true but email.token is unset. A locked mailer needs \
                     a provider token; set email.token or unset email.lock."
                );
            }
        }

        // Zero retention means data piles up forever -- probably not intended
        if self.storage.retention_days == 0 {
            tracing::warn!("storage.retention_days is 0 -- data will never be cleaned up");
        }

        self.validate_auth()?;

        Ok(())
    }

    /// OAuth/MCP validation -- a mix of hard errors (insecure issuer URL,
    /// silently-empty audience) and warnings for half-configured sections.
    fn validate_auth(&self) -> Result<()> {
        let oauth = &self.auth.oauth;

        if oauth.is_partially_configured() && !oauth.is_enabled() {
            tracing::warn!(
                "auth.oauth is partially configured -- issuer_url, client_id, client_secret \
                 and redirect_uri must all be set for OAuth to be enabled. Falling back to \
                 admin_token-only mode."
            );
        }

        if let Some(ref issuer) = oauth.issuer_url {
            validate_issuer_url_scheme(issuer)?;
        }

        if oauth.refresh_token_max_ttl_secs > REFRESH_TOKEN_MAX_TTL_CAP_SECS {
            anyhow::bail!(
                "auth.oauth.refresh_token_max_ttl_secs is {} seconds, which exceeds the 90-day \
                 hard cap ({} seconds). Pick a value <= 90 days; long-lived refresh tokens \
                 mostly indicate a misconfigured IdP, not a real session-length need.",
                oauth.refresh_token_max_ttl_secs,
                REFRESH_TOKEN_MAX_TTL_CAP_SECS
            );
        }

        if oauth.is_enabled() && oauth.web_audience.is_empty() {
            if !oauth.allow_empty_web_audience {
                anyhow::bail!(
                    "auth.oauth.web_audience is empty -- set it to the audience your IdP issues \
                     for the web client (e.g. \"stackpit-web\"), or set \
                     auth.oauth.allow_empty_web_audience = true to acknowledge that web-side \
                     audience checks are disabled (confused-deputy risk)."
                );
            }
            // Warn on every boot so the operator can't forget.
            tracing::warn!(
                "DEPRECATION: `auth.oauth.allow_empty_web_audience = true` will be removed in \
                 the next release; configure `auth.oauth.web_audience` to bind audience and \
                 prevent confused-deputy attacks across resource servers."
            );
        }

        if self.auth.mcp.is_enabled() && !oauth.is_enabled() {
            tracing::warn!(
                "auth.mcp is configured but auth.oauth is not -- the MCP gate pulls the issuer + \
                 JWKS from the OAuth discovery doc. MCP endpoint will stay disabled until \
                 [auth.oauth] is configured."
            );
        }

        if oauth.cache_max_ttl_secs > BEARER_CACHE_MAX_TTL_CAP_SECS {
            anyhow::bail!(
                "auth.oauth.cache_max_ttl_secs is {}, which exceeds the {}s cap. \
                 Pick a value <= {}; longer ceilings let stale scope / audience changes \
                 at the IdP linger past the next rotation.",
                oauth.cache_max_ttl_secs,
                BEARER_CACHE_MAX_TTL_CAP_SECS,
                BEARER_CACHE_MAX_TTL_CAP_SECS
            );
        }
        // JWKS TTL floor: sub-minute values degrade to refetch-per-request
        // (self-DoS). Reject at startup rather than letting the cache clamp
        // it silently.
        const JWKS_TTL_FLOOR_SECS: u64 = 60;
        if self.auth.mcp.is_enabled() && self.auth.mcp.jwks_cache_ttl_secs < JWKS_TTL_FLOOR_SECS {
            anyhow::bail!(
                "auth.mcp.jwks_cache_ttl_secs is {} (below the {}s floor). \
                 Sub-minute TTLs cause refetch-per-validation against the IdP's JWKS \
                 endpoint and amplify any transient outage into 401s for every MCP request. \
                 Pick a value >= {}; defaults are fine for almost every deployment.",
                self.auth.mcp.jwks_cache_ttl_secs,
                JWKS_TTL_FLOOR_SECS,
                JWKS_TTL_FLOOR_SECS,
            );
        }

        if self.auth.mcp.cache_max_ttl_secs > BEARER_CACHE_MAX_TTL_CAP_SECS {
            anyhow::bail!(
                "auth.mcp.cache_max_ttl_secs is {}, which exceeds the {}s cap. \
                 Pick a value <= {}; longer ceilings let stale scope / audience changes \
                 at the IdP linger past the next rotation.",
                self.auth.mcp.cache_max_ttl_secs,
                BEARER_CACHE_MAX_TTL_CAP_SECS,
                BEARER_CACHE_MAX_TTL_CAP_SECS
            );
        }

        if self.auth.mcp.is_enabled() && self.auth.mcp.audience.as_deref() == Some("") {
            tracing::warn!(
                "auth.mcp.audience is set to an empty string -- effectively disables audience \
                 binding for MCP tokens. Set it to the public MCP URL (matches Hydra's audience \
                 allowlist on the client)."
            );
        }

        Ok(())
    }
}
