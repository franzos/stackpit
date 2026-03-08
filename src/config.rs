use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub filter: FilterConfig,
    #[serde(default)]
    pub notifications: NotificationsConfig,
}

#[derive(Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_ingest_bind")]
    pub ingest_bind: String,
    /// Base URL for DSN generation. If not set, we fall back to `http://{ingest_bind}`.
    pub external_url: Option<String>,
    /// Shared token for the admin UI. If set, all admin routes require it
    /// via Bearer header or `stackpit_token` cookie.
    pub admin_token: Option<String>,
    /// Max request body size in bytes (applied after decompression).
    #[serde(default = "default_max_body_size")]
    pub max_body_size: usize,
    /// Max compressed request body size in bytes (applied before decompression).
    /// Defaults to max_body_size / 5 to guard against decompression bombs.
    pub max_compressed_body_size: Option<usize>,
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

impl std::fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConfig")
            .field("bind", &self.bind)
            .field("ingest_bind", &self.ingest_bind)
            .field("external_url", &self.external_url)
            .field(
                "admin_token",
                &self.admin_token.as_ref().map(|_| "[redacted]"),
            )
            .field("max_body_size", &self.max_body_size)
            .field("max_compressed_body_size", &self.compressed_body_limit())
            .finish()
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: DEFAULT_BIND.to_string(),
            ingest_bind: DEFAULT_INGEST_BIND.to_string(),
            external_url: None,
            admin_token: None,
            max_body_size: DEFAULT_MAX_BODY_SIZE,
            max_compressed_body_size: None,
        }
    }
}

impl ServerConfig {
    /// Effective compressed body limit: explicit value or max_body_size / 5.
    pub fn compressed_body_limit(&self) -> usize {
        self.max_compressed_body_size
            .unwrap_or(self.max_body_size / 5)
    }

    /// The base URL for DSN strings -- prefers `external_url` when set.
    pub fn dsn_base(&self) -> String {
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

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let contents = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&contents)?)
        } else {
            tracing::info!(
                "config file not found at {}, using defaults",
                path.display()
            );
            Ok(Self {
                server: ServerConfig::default(),
                storage: StorageConfig::default(),
                filter: FilterConfig::default(),
                notifications: NotificationsConfig::default(),
            })
        }
    }

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
            let trimmed = token.trim();
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

        // Refuse to expose an unprotected admin UI on a non-loopback address
        if self.server.admin_token.is_none() {
            if let Ok(addr) = self.server.bind.parse::<SocketAddr>() {
                if !addr.ip().is_loopback() {
                    anyhow::bail!(
                        "admin_token is not set but admin UI is bound to non-loopback address {addr}. \
                         Set server.admin_token in stackpit.toml or bind to 127.0.0.1."
                    );
                }
            }
        }

        // Zero retention means data piles up forever -- probably not intended
        if self.storage.retention_days == 0 {
            tracing::warn!("storage.retention_days is 0 -- data will never be cleaned up");
        }

        Ok(())
    }
}
