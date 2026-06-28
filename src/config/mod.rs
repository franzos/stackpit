use anyhow::Result;
use secrecy::SecretString;
use serde::Deserialize;
use std::path::Path;

mod url;
mod validate;

pub const DEFAULT_CONFIG_PATH: &str = "stackpit.toml";

/// Treat a blank/whitespace TOML string as absent so `from_address = ""` reads
/// the same as omitting it (otherwise `lock` validation would accept an empty sender).
fn empty_string_as_none<'de, D>(de: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(de)?.filter(|s| !s.trim().is_empty()))
}

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub filter: FilterConfig,
    #[serde(default)]
    pub notifications: NotificationsConfig,
    #[serde(default)]
    pub email: EmailConfig,
    #[serde(default)]
    pub auth: AuthConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_ingest_bind")]
    pub ingest_bind: String,
    /// Public URL of the admin/UI surface. Used for OIDC redirect_uri,
    /// post-logout, cookie Secure-flag heuristics, and (when
    /// `external_ingest_url` is unset) as the DSN host.
    pub external_url: Option<String>,
    /// Public URL of the **ingest** surface. Only needed when ingest is
    /// reachable on a different host or port than the admin URL (e.g. local
    /// split-port dev, split deployments). Falls back to `external_url`, then
    /// `http://{ingest_bind}`. This is the host:port that ends up in every
    /// DSN.
    pub external_ingest_url: Option<String>,
    /// Shared token for the admin UI. If set, all admin routes require it
    /// via Bearer header or `stackpit_token` cookie.
    pub admin_token: Option<SecretString>,
    /// AES-256 master key (64 hex chars) for at-rest secret encryption.
    /// `STACKPIT_MASTER_KEY` env var overrides this.
    #[serde(default)]
    pub master_key: Option<SecretString>,
    /// Max request body size in bytes (applied after decompression).
    #[serde(default = "default_max_body_size")]
    pub max_body_size: usize,
    /// Max compressed request body size in bytes (applied before decompression).
    /// Defaults to max_body_size / 5 to guard against decompression bombs.
    pub max_compressed_body_size: Option<usize>,
    /// Force `Secure` on cookies regardless of `external_url` scheme. Set
    /// behind a TLS-terminating proxy when the local listener is plain HTTP.
    #[serde(default)]
    pub force_secure_cookies: bool,
    /// Explicit acknowledgement of no-auth on a loopback bind. Without
    /// `admin_token` and `[auth.oauth]`, startup refuses unless this is set
    /// AND the bind is loopback. `stackpit init` provisions an admin_token
    /// so first-run never needs this.
    #[serde(default)]
    pub no_auth_loopback_acknowledged: bool,
}

#[derive(Debug, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_db_path")]
    pub path: String,
    /// Full database URL (e.g. `sqlite:stackpit.db` or `postgres://...`).
    /// Takes precedence over `path` when set.
    pub database_url: Option<String>,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

#[derive(Debug, Default, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RegistrationMode {
    #[default]
    Open,
    Closed,
}

#[derive(Debug, Default, Deserialize)]
pub struct FilterConfig {
    #[serde(default)]
    pub mode: RegistrationMode,
    #[serde(default)]
    pub rate_limit: u32,
    #[serde(default)]
    pub excluded_environments: Vec<String>,
    #[serde(default)]
    pub blocked_user_agents: Vec<String>,
    #[serde(default = "default_max_projects")]
    pub max_projects: usize,
}

#[derive(Debug, Default, Deserialize)]
pub struct NotificationsConfig {
    /// Max notifications per project per 60-second window. 0 = unlimited.
    #[serde(default)]
    pub rate_limit_per_project: u32,
    /// Max total notifications per 60-second window. 0 = unlimited.
    #[serde(default)]
    pub rate_limit_global: u32,
}

#[derive(Debug, Default, Deserialize)]
pub struct EmailConfig {
    /// Pre-selects the provider for new integrations; when `lock` is set this
    /// is the only provider used (the token comes from `STACKPIT_EMAIL_TOKEN`).
    #[serde(default)]
    pub provider: crate::providers::email::EmailProvider,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub from_address: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub from_name: Option<String>,
    /// Global provider API token. Same posture as `[server] admin_token` and
    /// `[auth.oauth] client_secret`: kept in config rather than DB-encrypted
    /// because it's a single instance-wide secret.
    #[serde(default)]
    pub token: Option<SecretString>,
    /// Lock sender + provider to this config: integrations only pick recipients.
    #[serde(default)]
    pub lock: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub oauth: OAuthConfig,
    #[serde(default)]
    pub mcp: McpConfig,
}

/// OIDC/OAuth2 client config (all optional; None disables OAuth).
#[derive(Debug, Default, Deserialize)]
pub struct OAuthConfig {
    pub issuer_url: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<SecretString>,
    pub redirect_uri: Option<String>,
    /// Post-logout redirect target. Must be registered on the OAuth2 client
    /// (Hydra enforces exact match). Unset → land on `/web/login`.
    pub post_logout_redirect_uri: Option<String>,
    /// Allow `post_logout_redirect_uri` on a different origin than
    /// `external_url`. Defaults `false` to block confused-deputy bounces
    /// via Hydra's `end_session_endpoint` query parameter.
    #[serde(default)]
    pub post_logout_allow_cross_origin: bool,
    /// Ceiling for back-channel revocation marker TTL (seconds). Must be
    /// at least Hydra's `lifespans.access_token` or markers time out while
    /// live tokens still claim the revoked sid/sub. Default 24h.
    #[serde(default = "default_access_token_max_ttl")]
    pub access_token_max_ttl_secs: u64,
    /// Ceiling for refresh-token revocation marker TTL (seconds). Must be
    /// at least Hydra's `lifespans.refresh_token` or a revoked grant can
    /// outlive the marker and get re-armed on replay. Hydra omits
    /// `refresh_token_exp`, so the operator pins it here. Default 14d;
    /// hard-capped at 90d.
    #[serde(default = "default_refresh_token_max_ttl")]
    pub refresh_token_max_ttl_secs: u64,
    /// Expected `aud` on the web bearer gate. Empty = skip audience binding;
    /// only safe when no other resource server shares the IdP.
    #[serde(default)]
    pub web_audience: String,
    /// Required scope on every web bearer-gate authorization. Empty = accept
    /// any introspection-valid token. Set to e.g. `stackpit:web` for
    /// defense-in-depth against tokens reused from other resource servers.
    #[serde(default)]
    pub web_required_scope: String,
    /// Web introspection-cache TTL (seconds).
    #[serde(default = "default_introspection_cache_ttl")]
    pub introspection_cache_ttl_secs: u64,
    /// Hard ceiling on any cached bearer entry's TTL (seconds). Bounds how
    /// long stale IdP scope/audience changes stay served. `0` disables the
    /// cache; startup rejects values above 300.
    #[serde(default = "default_bearer_cache_max_ttl")]
    pub cache_max_ttl_secs: u64,
    /// Web bearer-gate introspection endpoint. Defaults to MCP's value.
    pub introspection_url: Option<String>,
    /// `true` = fail startup on discovery failure. `false` = log + fall back
    /// to admin-token-only.
    #[serde(default)]
    pub required: bool,
    /// Opt in to startup proceeding when the IdP discovery doc omits
    /// `end_session_endpoint` (OIDC RP-Initiated Logout 1.0 §3). Without
    /// this, `required = true` refuses to start and `required = false` warns
    /// every boot. Set to silence the warning on IdPs without RP-initiated
    /// logout (Stackpit-only logout; the IdP session survives).
    #[serde(default)]
    pub allow_local_only_logout: bool,
    /// Override the token-endpoint client-auth method. Unset = negotiate from
    /// discovery (prefers client_secret_basic). Accepts `client_secret_basic`
    /// (alias `basic`) or `client_secret_post` (alias `post`).
    pub token_endpoint_auth_method: Option<String>,
}

fn default_access_token_max_ttl() -> u64 {
    24 * 3600
}

fn default_refresh_token_max_ttl() -> u64 {
    14 * 24 * 3600
}

/// MCP bearer-auth config. `audience = None` disables MCP. Requires
/// `[auth.oauth]`; the dispatcher pulls the issuer from there.
///
/// Accepts RS256 JWTs against the IdP's JWKS; opaque tokens fall through
/// to introspection. At least one validation path must be reachable.
#[derive(Debug, Default, Deserialize)]
pub struct McpConfig {
    pub audience: Option<String>,
    /// Falls back to discovery's `introspection_endpoint` when unset.
    pub introspection_url: Option<String>,
    /// JWKS endpoint override (rare; for mirror/proxy scenarios).
    pub jwks_url: Option<String>,
    /// JWKS cache TTL (seconds). Default 24h. Refetches on `kid` miss regardless.
    #[serde(default = "default_jwks_cache_ttl")]
    pub jwks_cache_ttl_secs: u64,
    /// Basic-auth for the introspection POST. Required against Hydra's
    /// public `/oauth2/introspect`.
    pub introspection_client_id: Option<String>,
    pub introspection_client_secret: Option<SecretString>,
    /// Positive-cache TTL (seconds). Keyed on SHA-256(token); only accepted
    /// tokens are stored, capped at `min(ttl, exp-now)`. `0` disables. Default 60.
    #[serde(default = "default_introspection_cache_ttl")]
    pub introspection_cache_ttl_secs: u64,
    /// Hard ceiling on any cached bearer entry's TTL (seconds). `0`
    /// disables; startup rejects values above 300.
    #[serde(default = "default_bearer_cache_max_ttl")]
    pub cache_max_ttl_secs: u64,
}

const DEFAULT_BIND: &str = "127.0.0.1:3000";
const DEFAULT_INGEST_BIND: &str = "0.0.0.0:3001";
const DEFAULT_DB_PATH: &str = "stackpit.db";
const DEFAULT_RETENTION_DAYS: u32 = 90;
const DEFAULT_MAX_PROJECTS: usize = 1000;
const DEFAULT_MAX_BODY_SIZE: usize = 10 * 1024 * 1024; // 10MB

fn default_bind() -> String {
    DEFAULT_BIND.to_string()
}
fn default_ingest_bind() -> String {
    DEFAULT_INGEST_BIND.to_string()
}
fn default_db_path() -> String {
    DEFAULT_DB_PATH.to_string()
}
fn default_retention_days() -> u32 {
    DEFAULT_RETENTION_DAYS
}
fn default_max_projects() -> usize {
    DEFAULT_MAX_PROJECTS
}
fn default_max_body_size() -> usize {
    DEFAULT_MAX_BODY_SIZE
}
fn default_introspection_cache_ttl() -> u64 {
    60
}
fn default_jwks_cache_ttl() -> u64 {
    24 * 3600
}
fn default_bearer_cache_max_ttl() -> u64 {
    30
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: DEFAULT_BIND.to_string(),
            ingest_bind: DEFAULT_INGEST_BIND.to_string(),
            external_url: None,
            external_ingest_url: None,
            admin_token: None,
            master_key: None,
            max_body_size: DEFAULT_MAX_BODY_SIZE,
            max_compressed_body_size: None,
            force_secure_cookies: false,
            no_auth_loopback_acknowledged: false,
        }
    }
}

impl ServerConfig {
    /// Effective compressed body limit: explicit value or max_body_size / 5.
    pub fn compressed_body_limit(&self) -> usize {
        self.max_compressed_body_size
            .unwrap_or(self.max_body_size / 5)
    }

    /// The base URL for DSN strings: prefer `external_ingest_url`, then
    /// `external_url`, then the raw `ingest_bind`. Keeping the precedence
    /// here means SDKs always get a URL that hits the ingest listener.
    pub fn dsn_base(&self) -> String {
        if let Some(url) = &self.external_ingest_url {
            return url.trim_end_matches('/').to_string();
        }
        match &self.external_url {
            Some(url) => url.trim_end_matches('/').to_string(),
            None => format!("http://{}", self.ingest_bind),
        }
    }

    /// Assembles a full DSN from a public key and project ID.
    pub fn build_dsn(&self, public_key: &str, project_id: u64) -> String {
        let base = self.dsn_base();
        let (scheme, host) = base.split_once("://").unwrap_or(("http", &base));
        format!("{scheme}://{public_key}@{host}/{project_id}")
    }

    /// Canonical Secure-cookie check. Call from every cookie-setting path.
    pub fn cookies_should_be_secure(&self) -> bool {
        self.force_secure_cookies
            || self
                .external_url
                .as_deref()
                .is_some_and(|u| u.starts_with("https://"))
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: DEFAULT_DB_PATH.to_string(),
            database_url: None,
            retention_days: DEFAULT_RETENTION_DAYS,
        }
    }
}

impl StorageConfig {
    /// Resolve the database URL: `database_url` takes precedence, otherwise
    /// the `path` field is converted to a `sqlite:` URL.
    pub fn database_url(&self) -> String {
        crate::db::pool::resolve_database_url(self.database_url.as_deref(), &self.path)
    }
}

impl OAuthConfig {
    /// OAuth is considered enabled iff every required field is set.
    pub fn is_enabled(&self) -> bool {
        self.issuer_url.is_some()
            && self.client_id.is_some()
            && self.client_secret.is_some()
            && self.redirect_uri.is_some()
    }

    /// True if at least one field is set; used to detect partial config.
    pub fn is_partially_configured(&self) -> bool {
        self.issuer_url.is_some()
            || self.client_id.is_some()
            || self.client_secret.is_some()
            || self.redirect_uri.is_some()
    }
}

impl McpConfig {
    /// MCP gate is "configured" when an audience is set. Reachability is
    /// validated separately at startup.
    pub fn is_enabled(&self) -> bool {
        self.audience.is_some()
    }

    /// Clamped at 10m: revocation latency matters more than a few cache hits.
    pub fn effective_cache_ttl_secs(&self) -> u64 {
        const MAX_TTL_SECS: u64 = 600;
        self.introspection_cache_ttl_secs.min(MAX_TTL_SECS)
    }
}

impl Config {
    pub fn load(path: &Path, explicit: bool) -> Result<Self> {
        if path.exists() {
            let contents = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&contents)?)
        } else if explicit {
            anyhow::bail!(
                "config file '{}' not found (explicitly requested via --config)",
                path.display()
            );
        } else {
            tracing::info!(
                "config file not found at {}, using defaults",
                path.display()
            );
            Ok(Self::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loopback_oauth_enabled() -> Config {
        Config {
            server: ServerConfig {
                bind: "127.0.0.1:3000".to_string(),
                ingest_bind: "127.0.0.1:3001".to_string(),
                ..ServerConfig::default()
            },
            storage: StorageConfig::default(),
            filter: FilterConfig::default(),
            notifications: NotificationsConfig::default(),
            email: EmailConfig::default(),
            auth: AuthConfig {
                oauth: OAuthConfig {
                    issuer_url: Some("https://idp.example.com".to_string()),
                    client_id: Some("stackpit".to_string()),
                    client_secret: Some(SecretString::from("secret-value")),
                    redirect_uri: Some(
                        "https://stackpit.example.com/web/auth/callback".to_string(),
                    ),
                    web_audience: "stackpit-web".to_string(),
                    refresh_token_max_ttl_secs: default_refresh_token_max_ttl(),
                    access_token_max_ttl_secs: default_access_token_max_ttl(),
                    introspection_cache_ttl_secs: 60,
                    ..OAuthConfig::default()
                },
                mcp: McpConfig::default(),
            },
        }
    }

    #[test]
    fn refresh_ttl_default_is_fourteen_days() {
        let cfg: OAuthConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.refresh_token_max_ttl_secs, 14 * 24 * 3600);
    }

    #[test]
    fn refresh_ttl_at_ninety_days_validates() {
        let mut cfg = loopback_oauth_enabled();
        cfg.auth.oauth.refresh_token_max_ttl_secs = 90 * 24 * 3600;
        cfg.validate().expect("90d should validate");
    }

    #[test]
    fn refresh_ttl_above_ninety_days_rejected() {
        let mut cfg = loopback_oauth_enabled();
        cfg.auth.oauth.refresh_token_max_ttl_secs = 91 * 24 * 3600;
        let err = cfg.validate().expect_err("91d must be rejected");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("90-day"),
            "error should mention the cap; got: {msg}"
        );
    }

    fn no_auth_config(bind: &str, ack: bool) -> Config {
        Config {
            server: ServerConfig {
                bind: bind.to_string(),
                ingest_bind: "127.0.0.1:3001".to_string(),
                admin_token: None,
                no_auth_loopback_acknowledged: ack,
                ..ServerConfig::default()
            },
            storage: StorageConfig::default(),
            filter: FilterConfig::default(),
            notifications: NotificationsConfig::default(),
            email: EmailConfig::default(),
            auth: AuthConfig::default(),
        }
    }

    #[test]
    fn no_auth_non_loopback_rejected() {
        let cfg = no_auth_config("0.0.0.0:3000", false);
        let err = cfg.validate().expect_err("non-loopback no-auth must fail");
        assert!(format!("{err:#}").contains("no auth mode configured"));
    }

    #[test]
    fn no_auth_loopback_without_ack_rejected() {
        let cfg = no_auth_config("127.0.0.1:3000", false);
        let err = cfg
            .validate()
            .expect_err("loopback no-auth without ack must fail");
        assert!(format!("{err:#}").contains("no_auth_loopback_acknowledged"));
    }

    #[test]
    fn no_auth_loopback_with_ack_ok() {
        let cfg = no_auth_config("127.0.0.1:3000", true);
        cfg.validate()
            .expect("loopback no-auth with explicit ack should validate");
    }

    #[test]
    fn empty_web_audience_rejected() {
        let mut cfg = loopback_oauth_enabled();
        cfg.auth.oauth.web_audience = String::new();
        let err = cfg.validate().expect_err("empty audience must fail");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("web_audience"),
            "error should mention web_audience; got: {msg}"
        );
    }

    #[test]
    fn populated_web_audience_validates() {
        let mut cfg = loopback_oauth_enabled();
        cfg.auth.oauth.web_audience = "stackpit-web".to_string();
        cfg.validate().expect("populated audience should pass");
    }

    #[test]
    fn post_logout_same_origin_validates() {
        let mut cfg = loopback_oauth_enabled();
        cfg.server.external_url = Some("https://stackpit.example.com".to_string());
        cfg.auth.oauth.post_logout_redirect_uri =
            Some("https://stackpit.example.com/web/login".to_string());
        cfg.validate().expect("same-origin post-logout should pass");
    }

    #[test]
    fn post_logout_cross_origin_without_flag_rejected() {
        let mut cfg = loopback_oauth_enabled();
        cfg.server.external_url = Some("https://stackpit.example.com".to_string());
        cfg.auth.oauth.post_logout_redirect_uri =
            Some("https://other.example.com/landing".to_string());
        let err = cfg
            .validate()
            .expect_err("cross-origin without opt-in must fail");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("post_logout_allow_cross_origin"),
            "error should point at the flag; got: {msg}"
        );
    }

    #[test]
    fn post_logout_cross_origin_with_flag_validates() {
        let mut cfg = loopback_oauth_enabled();
        cfg.server.external_url = Some("https://stackpit.example.com".to_string());
        cfg.auth.oauth.post_logout_redirect_uri =
            Some("https://other.example.com/landing".to_string());
        cfg.auth.oauth.post_logout_allow_cross_origin = true;
        cfg.validate()
            .expect("cross-origin with explicit opt-in should pass");
    }

    #[test]
    fn post_logout_same_origin_with_flag_still_validates() {
        // Same-origin: the flag is irrelevant either way.
        let mut cfg = loopback_oauth_enabled();
        cfg.server.external_url = Some("https://stackpit.example.com".to_string());
        cfg.auth.oauth.post_logout_redirect_uri =
            Some("https://stackpit.example.com/web/login".to_string());
        cfg.auth.oauth.post_logout_allow_cross_origin = true;
        cfg.validate()
            .expect("same-origin with flag set should still pass");
    }

    #[test]
    fn non_loopback_with_ack_still_rejected() {
        // Ack alone doesn't unlock non-loopback. Belt-and-suspenders against
        // an operator who flips the flag and forgets to bind to loopback.
        let cfg = no_auth_config("0.0.0.0:3000", true);
        let err = cfg
            .validate()
            .expect_err("ack on non-loopback must still fail");
        assert!(format!("{err:#}").contains("no auth mode configured"));
    }
}
