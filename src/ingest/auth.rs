//! Project key validation with an in-memory cache in front of the DB.
//! Open mode auto-provisions a new project (with its first key) on an unknown
//! project_id; once a project exists, only registered keys are accepted.
//! Closed mode requires every project_id+key to exist.
//! HTTP-level handling (headers, responses) lives in `endpoints::auth`.

use crate::config::RegistrationMode;
use crate::domain::ProjectStatus;
use crate::queries;
use crate::server::AppState;
use axum::http::HeaderMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SentryAuth {
    pub sentry_key: String,
}

/// Extract the sentry key from request headers (X-Sentry-Auth, then Authorization).
pub fn extract_from_header(headers: &HeaderMap) -> Option<SentryAuth> {
    let header_val = headers
        .get("X-Sentry-Auth")
        .or_else(|| headers.get("Authorization"))
        .and_then(|v| v.to_str().ok())?;

    parse_auth_header(header_val)
}

pub fn extract_from_query(query: Option<&str>) -> Option<SentryAuth> {
    let query = query?;
    form_urlencoded::parse(query.as_bytes())
        .find(|(k, _)| k == "sentry_key")
        .map(|(_, v)| SentryAuth {
            sentry_key: v.into_owned(),
        })
}

/// Parse a DSN string into its auth key and project ID.
pub fn extract_from_dsn(dsn: &str) -> Option<(SentryAuth, u64)> {
    let without_scheme = dsn
        .strip_prefix("https://")
        .or_else(|| dsn.strip_prefix("http://"))?;
    let (key, rest) = without_scheme.split_once('@')?;
    let project_str = rest.rsplit('/').find(|s| !s.is_empty())?;
    let project_id: u64 = project_str.parse().ok()?;
    Some((
        SentryAuth {
            sentry_key: key.to_string(),
        },
        project_id,
    ))
}

fn parse_auth_header(value: &str) -> Option<SentryAuth> {
    let payload = value
        .strip_prefix("Sentry ")
        .or_else(|| value.strip_prefix("sentry "))?;

    let mut sentry_key = None;

    for part in payload.split(',') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("sentry_key=") {
            sentry_key = Some(val.to_string());
        }
    }

    Some(SentryAuth {
        sentry_key: sentry_key?,
    })
}

pub struct CacheEntry {
    pub project_id: u64,
    pub status: ProjectStatus,
    pub inserted_at: std::time::Instant,
}

pub type AuthCache = Arc<dashmap::DashMap<String, CacheEntry>>;

pub const AUTH_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// Drops all cached entries for a project (call when project settings change).
pub fn invalidate_project(cache: &AuthCache, project_id: u64) {
    cache.retain(|_, entry| entry.project_id != project_id);
}

pub fn invalidate_key(cache: &AuthCache, key: &str) {
    cache.remove(key);
}

const AUTH_CACHE_MAX_ENTRIES: usize = 50_000;

pub enum AuthError {
    Archived,
    Denied(&'static str),
    MaxProjects,
    InternalError,
}

/// Checks a sentry key against the cache first, falls back to DB on miss.
/// In open mode, unknown keys get auto-registered on the fly.
pub async fn validate_project_key(
    state: &AppState,
    sentry_key: &str,
    project_id: u64,
) -> Result<(), AuthError> {
    // Compute all comparisons before branching to avoid leaking info through timing.
    if let Some(entry) = state.auth_cache.get(sentry_key) {
        let cached = entry.value();
        if cached.inserted_at.elapsed() < AUTH_CACHE_TTL {
            let is_archived = cached.status.is_archived();
            let project_matches = cached.project_id == project_id;

            if is_archived {
                return Err(AuthError::Archived);
            }
            if !project_matches {
                let msg = match state.config.filter.mode {
                    RegistrationMode::Closed => "project or key denied",
                    RegistrationMode::Open => "key/project mismatch",
                };
                return Err(AuthError::Denied(msg));
            }
            return Ok(());
        }
        // Evict only if still expired so a concurrent fresh insert isn't clobbered.
        drop(entry);
        state
            .auth_cache
            .remove_if(sentry_key, |_, e| e.inserted_at.elapsed() >= AUTH_CACHE_TTL);
    }

    let pool = &state.pool;

    if let Ok(Some(status)) = queries::projects::get_project_status(pool, project_id).await {
        if status.is_archived() {
            return Err(AuthError::Archived);
        }
    }

    // Prune expired entries when the cache gets big.
    if state.auth_cache.len() > AUTH_CACHE_MAX_ENTRIES {
        state
            .auth_cache
            .retain(|_, entry| entry.inserted_at.elapsed() < AUTH_CACHE_TTL);
    }

    match state.config.filter.mode {
        RegistrationMode::Closed => {
            match queries::projects::get_project_key(pool, sentry_key).await {
                Ok(Some(key))
                    if key.status == ProjectStatus::Active && key.project_id == project_id =>
                {
                    state.auth_cache.insert(
                        sentry_key.to_owned(),
                        CacheEntry {
                            project_id,
                            status: ProjectStatus::Active,
                            inserted_at: std::time::Instant::now(),
                        },
                    );
                }
                _ => {
                    return Err(AuthError::Denied("project or key denied"));
                }
            }
        }
        RegistrationMode::Open => {
            match queries::projects::get_project_key(pool, sentry_key).await {
                Ok(Some(key)) => {
                    if key.project_id != project_id {
                        return Err(AuthError::Denied("key/project mismatch"));
                    }
                    state.auth_cache.insert(
                        sentry_key.to_owned(),
                        CacheEntry {
                            project_id,
                            status: ProjectStatus::Active,
                            inserted_at: std::time::Instant::now(),
                        },
                    );
                }
                Ok(None) => {
                    // First DSN wins: auto-provision only when the project doesn't exist yet,
                    // else a client could mint a key by guessing project_id with random hex.
                    let project_exists = queries::projects::get_project_status(pool, project_id)
                        .await
                        .ok()
                        .flatten()
                        .is_some();
                    if project_exists {
                        return Err(AuthError::Denied("unknown key for existing project"));
                    }
                    let project_count = queries::projects::count_distinct_projects(pool)
                        .await
                        .unwrap_or(0);
                    if project_count >= state.config.filter.max_projects {
                        tracing::warn!(
                            "open mode: max projects ({}) reached, rejecting unknown key",
                            state.config.filter.max_projects
                        );
                        return Err(AuthError::MaxProjects);
                    }
                    auto_register_key(state, sentry_key, project_id).await;
                }
                Err(e) => {
                    tracing::warn!("open-mode auth: DB lookup failed: {e}");
                    return Err(AuthError::InternalError);
                }
            }
        }
    }

    Ok(())
}

/// Commits the project/key row on the writer pool so it serialises with the
/// actor before any events referencing it can be flushed.
async fn auto_register_key(state: &AppState, sentry_key: &str, project_id: u64) {
    match queries::projects::ensure_project_key(&state.writer_pool, project_id, sentry_key).await {
        Ok(()) => {
            state.auth_cache.insert(
                sentry_key.to_owned(),
                CacheEntry {
                    project_id,
                    status: ProjectStatus::Active,
                    inserted_at: std::time::Instant::now(),
                },
            );
        }
        Err(e) => tracing::warn!("auto-register key failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn parse_auth_header_with_key_and_version() {
        let auth = parse_auth_header("Sentry sentry_key=abc123, sentry_version=7").unwrap();
        assert_eq!(auth.sentry_key, "abc123");
    }

    #[test]
    fn parse_auth_header_lowercase_prefix() {
        let auth = parse_auth_header("sentry sentry_key=key1").unwrap();
        assert_eq!(auth.sentry_key, "key1");
    }

    #[test]
    fn parse_auth_header_missing_prefix_returns_none() {
        assert!(parse_auth_header("Bearer token123").is_none());
    }

    #[test]
    fn parse_auth_header_missing_key_returns_none() {
        assert!(parse_auth_header("Sentry sentry_version=7").is_none());
    }

    #[test]
    fn extract_from_header_x_sentry_auth() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Sentry-Auth", "Sentry sentry_key=abc".parse().unwrap());
        let auth = extract_from_header(&headers).unwrap();
        assert_eq!(auth.sentry_key, "abc");
    }

    #[test]
    fn extract_from_header_authorization_fallback() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Sentry sentry_key=xyz".parse().unwrap());
        let auth = extract_from_header(&headers).unwrap();
        assert_eq!(auth.sentry_key, "xyz");
    }

    #[test]
    fn extract_from_header_missing_returns_none() {
        let headers = HeaderMap::new();
        assert!(extract_from_header(&headers).is_none());
    }

    #[test]
    fn extract_from_query_valid() {
        let auth = extract_from_query(Some("sentry_key=mykey&other=1")).unwrap();
        assert_eq!(auth.sentry_key, "mykey");
    }

    #[test]
    fn extract_from_query_url_encoded_key() {
        let auth = extract_from_query(Some("sentry_key=abc%3D123%26key")).unwrap();
        assert_eq!(auth.sentry_key, "abc=123&key");
    }

    #[test]
    fn extract_from_query_no_key() {
        assert!(extract_from_query(Some("foo=bar&baz=1")).is_none());
    }

    #[test]
    fn extract_from_query_none_input() {
        assert!(extract_from_query(None).is_none());
    }

    #[test]
    fn extract_from_dsn_https() {
        let (auth, project_id) =
            extract_from_dsn("https://abc123@o123.ingest.sentry.io/456").unwrap();
        assert_eq!(auth.sentry_key, "abc123");
        assert_eq!(project_id, 456);
    }

    #[test]
    fn extract_from_dsn_http() {
        let (auth, project_id) = extract_from_dsn("http://key@localhost:3000/42").unwrap();
        assert_eq!(auth.sentry_key, "key");
        assert_eq!(project_id, 42);
    }

    #[test]
    fn extract_from_dsn_invalid_scheme() {
        assert!(extract_from_dsn("ftp://key@host/1").is_none());
    }

    #[test]
    fn extract_from_dsn_no_project_id() {
        assert!(extract_from_dsn("https://key@host/notanumber").is_none());
    }

    #[test]
    fn extract_from_dsn_no_at_sign() {
        assert!(extract_from_dsn("https://noatsign/1").is_none());
    }

    #[test]
    fn extract_from_dsn_trailing_slash() {
        let (auth, project_id) = extract_from_dsn("https://key@host/42/").unwrap();
        assert_eq!(auth.sentry_key, "key");
        assert_eq!(project_id, 42);
    }
}
