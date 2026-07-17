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

/// A definitive denial worth caching so an unknown-key flood can't re-hit the DB.
#[derive(Clone, Copy)]
pub enum Denial {
    Archived,
    Denied(&'static str),
    MaxProjects,
}

impl From<Denial> for AuthError {
    fn from(d: Denial) -> Self {
        match d {
            Denial::Archived => AuthError::Archived,
            Denial::Denied(msg) => AuthError::Denied(msg),
            Denial::MaxProjects => AuthError::MaxProjects,
        }
    }
}

pub struct NegativeEntry {
    pub denial: Denial,
    pub inserted_at: std::time::Instant,
}

/// Keyed by `(sentry_key, project_id)` so a denial for one pair never masks a valid pair, and per-project/per-key invalidation stays exact.
pub type NegativeAuthCache = Arc<dashmap::DashMap<(String, u64), NegativeEntry>>;

/// Short so a key that later becomes valid is denied for at most this long.
pub const NEGATIVE_AUTH_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(45);

const NEGATIVE_AUTH_CACHE_MAX_ENTRIES: usize = 50_000;

/// Cached open-mode project count with a short TTL so the unknown-key path skips the `COUNT(DISTINCT)` most of the time; a few seconds of staleness only risks briefly over/under-admitting at the `max_projects` boundary.
pub type ProjectCountCache = Arc<parking_lot::Mutex<Option<(usize, std::time::Instant)>>>;

pub const PROJECT_COUNT_TTL: std::time::Duration = std::time::Duration::from_secs(15);

/// True while a cache entry of the given age is still within the TTL window.
fn cache_fresh(entry_age: std::time::Duration, ttl: std::time::Duration) -> bool {
    entry_age < ttl
}

/// Drops all cached entries (positive and negative) for a project; call when project settings change so an unarchive/registration clears stale denials.
pub fn invalidate_project(cache: &AuthCache, negative: &NegativeAuthCache, project_id: u64) {
    cache.retain(|_, entry| entry.project_id != project_id);
    negative.retain(|(_, pid), _| *pid != project_id);
}

pub fn invalidate_key(cache: &AuthCache, negative: &NegativeAuthCache, key: &str) {
    cache.remove(key);
    negative.retain(|(k, _), _| k != key);
}

fn negative_cache_insert(state: &AppState, sentry_key: &str, project_id: u64, denial: Denial) {
    let cache = &state.negative_auth_cache;
    if cache.len() > NEGATIVE_AUTH_CACHE_MAX_ENTRIES {
        cache.retain(|_, e| cache_fresh(e.inserted_at.elapsed(), NEGATIVE_AUTH_CACHE_TTL));
    }
    cache.insert(
        (sentry_key.to_owned(), project_id),
        NegativeEntry {
            denial,
            inserted_at: std::time::Instant::now(),
        },
    );
}

/// Cached open-mode project count. Recomputes and stores on a stale/absent entry.
async fn cached_project_count(state: &AppState) -> usize {
    {
        let guard = state.project_count_cache.lock();
        if let Some((count, at)) = guard.as_ref() {
            if cache_fresh(at.elapsed(), PROJECT_COUNT_TTL) {
                return *count;
            }
        }
    }
    let count = queries::projects::count_distinct_projects(&state.pool)
        .await
        .unwrap_or(0);
    *state.project_count_cache.lock() = Some((count, std::time::Instant::now()));
    count
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

    // Serve definitive denials from the negative cache without any DB query; a deliberate timing side channel, since absorbing an unknown-key DB flood is worth more than uniform response latency here.
    if let Some(entry) = state
        .negative_auth_cache
        .get(&(sentry_key.to_owned(), project_id))
    {
        if cache_fresh(entry.inserted_at.elapsed(), NEGATIVE_AUTH_CACHE_TTL) {
            return Err(entry.denial.into());
        }
    }

    let pool = &state.pool;

    // Single status lookup shared between the archived check and the open-mode project-exists check below.
    let project_status = project_status_checked(pool, project_id).await?;
    if let Some(status) = &project_status {
        if status.is_archived() {
            negative_cache_insert(state, sentry_key, project_id, Denial::Archived);
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
                    negative_cache_insert(
                        state,
                        sentry_key,
                        project_id,
                        Denial::Denied("project or key denied"),
                    );
                    return Err(AuthError::Denied("project or key denied"));
                }
            }
        }
        RegistrationMode::Open => {
            match queries::projects::get_project_key(pool, sentry_key).await {
                Ok(Some(key)) => {
                    if key.project_id != project_id {
                        negative_cache_insert(
                            state,
                            sentry_key,
                            project_id,
                            Denial::Denied("key/project mismatch"),
                        );
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
                    if project_status.is_some() {
                        negative_cache_insert(
                            state,
                            sentry_key,
                            project_id,
                            Denial::Denied("unknown key for existing project"),
                        );
                        return Err(AuthError::Denied("unknown key for existing project"));
                    }
                    let project_count = cached_project_count(state).await;
                    if project_count >= state.config.filter.max_projects {
                        tracing::warn!(
                            "open mode: max projects ({}) reached, rejecting unknown key",
                            state.config.filter.max_projects
                        );
                        negative_cache_insert(state, sentry_key, project_id, Denial::MaxProjects);
                        return Err(AuthError::MaxProjects);
                    }
                    auto_register_key(
                        &state.writer_pool,
                        &state.auth_cache,
                        sentry_key,
                        project_id,
                    )
                    .await?;
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

/// A failed status lookup must surface as a retryable 5xx: flattening it to `None` would let open mode auto-register an unknown key into an existing project and skip the archived check.
async fn project_status_checked(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<Option<ProjectStatus>, AuthError> {
    queries::projects::get_project_status(pool, project_id)
        .await
        .map_err(|e| {
            tracing::warn!("auth: project status lookup failed: {e}");
            AuthError::InternalError
        })
}

/// Commits the project/key row on the writer pool so it serialises with the
/// actor before any events referencing it can be flushed. A failed insert must
/// surface as an error so the SDK gets a retryable 5xx instead of a 200 for a
/// key that was never committed.
async fn auto_register_key(
    writer_pool: &crate::db::DbPool,
    auth_cache: &AuthCache,
    sentry_key: &str,
    project_id: u64,
) -> Result<(), AuthError> {
    match queries::projects::ensure_project_key(writer_pool, project_id, sentry_key).await {
        Ok(()) => {
            auth_cache.insert(
                sentry_key.to_owned(),
                CacheEntry {
                    project_id,
                    status: ProjectStatus::Active,
                    inserted_at: std::time::Instant::now(),
                },
            );
            Ok(())
        }
        Err(e) => {
            tracing::warn!("auto-register key failed: {e}");
            Err(AuthError::InternalError)
        }
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

    use std::time::{Duration, Instant};

    fn new_negative_cache() -> NegativeAuthCache {
        Arc::new(dashmap::DashMap::new())
    }

    fn stale_instant(secs: u64) -> Instant {
        Instant::now()
            .checked_sub(Duration::from_secs(secs))
            .expect("test clock underflow")
    }

    #[test]
    fn cache_fresh_respects_ttl() {
        assert!(cache_fresh(Duration::from_secs(5), Duration::from_secs(15)));
        assert!(!cache_fresh(
            Duration::from_secs(15),
            Duration::from_secs(15)
        ));
        assert!(!cache_fresh(
            Duration::from_secs(20),
            Duration::from_secs(15)
        ));
    }

    #[test]
    fn denial_roundtrips_to_auth_error() {
        assert!(matches!(
            AuthError::from(Denial::Archived),
            AuthError::Archived
        ));
        assert!(matches!(
            AuthError::from(Denial::Denied("x")),
            AuthError::Denied("x")
        ));
        assert!(matches!(
            AuthError::from(Denial::MaxProjects),
            AuthError::MaxProjects
        ));
    }

    #[test]
    fn negative_entry_fresh_within_ttl_and_expires() {
        let cache = new_negative_cache();
        cache.insert(
            ("k".to_owned(), 7),
            NegativeEntry {
                denial: Denial::Denied("nope"),
                inserted_at: Instant::now(),
            },
        );
        let entry = cache.get(&("k".to_owned(), 7)).unwrap();
        assert!(cache_fresh(
            entry.inserted_at.elapsed(),
            NEGATIVE_AUTH_CACHE_TTL
        ));
        drop(entry);

        cache.insert(
            ("k".to_owned(), 7),
            NegativeEntry {
                denial: Denial::Denied("nope"),
                inserted_at: stale_instant(NEGATIVE_AUTH_CACHE_TTL.as_secs() + 1),
            },
        );
        let entry = cache.get(&("k".to_owned(), 7)).unwrap();
        assert!(!cache_fresh(
            entry.inserted_at.elapsed(),
            NEGATIVE_AUTH_CACHE_TTL
        ));
    }

    #[test]
    fn invalidate_key_clears_negative_entries() {
        let positive: AuthCache = Arc::new(dashmap::DashMap::new());
        let negative = new_negative_cache();
        negative.insert(
            ("k".to_owned(), 1),
            NegativeEntry {
                denial: Denial::Archived,
                inserted_at: Instant::now(),
            },
        );
        negative.insert(
            ("other".to_owned(), 1),
            NegativeEntry {
                denial: Denial::Archived,
                inserted_at: Instant::now(),
            },
        );
        invalidate_key(&positive, &negative, "k");
        assert!(negative.get(&("k".to_owned(), 1)).is_none());
        assert!(negative.get(&("other".to_owned(), 1)).is_some());
    }

    // Mirrors the project-delete flow: a still-cached key for the deleted
    // project must not keep authenticating ingest until its TTL runs out.
    #[test]
    fn invalidate_project_clears_positive_entries() {
        let positive: AuthCache = Arc::new(dashmap::DashMap::new());
        let negative = new_negative_cache();
        positive.insert(
            "deleted_key".to_owned(),
            CacheEntry {
                project_id: 9,
                status: ProjectStatus::Active,
                inserted_at: Instant::now(),
            },
        );
        positive.insert(
            "other_key".to_owned(),
            CacheEntry {
                project_id: 10,
                status: ProjectStatus::Active,
                inserted_at: Instant::now(),
            },
        );
        invalidate_project(&positive, &negative, 9);
        assert!(positive.get("deleted_key").is_none());
        assert!(positive.get("other_key").is_some());
    }

    #[test]
    fn invalidate_project_clears_negative_entries() {
        let positive: AuthCache = Arc::new(dashmap::DashMap::new());
        let negative = new_negative_cache();
        negative.insert(
            ("k".to_owned(), 1),
            NegativeEntry {
                denial: Denial::MaxProjects,
                inserted_at: Instant::now(),
            },
        );
        negative.insert(
            ("k".to_owned(), 2),
            NegativeEntry {
                denial: Denial::MaxProjects,
                inserted_at: Instant::now(),
            },
        );
        invalidate_project(&positive, &negative, 1);
        assert!(negative.get(&("k".to_owned(), 1)).is_none());
        assert!(negative.get(&("k".to_owned(), 2)).is_some());
    }

    #[tokio::test]
    async fn auto_register_key_success_caches_key() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let cache: AuthCache = Arc::new(dashmap::DashMap::new());
        let result = auto_register_key(&pool, &cache, "newkey", 7).await;
        assert!(result.is_ok(), "insert into a live DB must succeed");
        let entry = cache.get("newkey").expect("key must be cached");
        assert_eq!(entry.project_id, 7);
    }

    // Regression: a failed auto-register insert must surface as InternalError
    // (retryable 5xx), not silently fall through to an accepted event.
    #[tokio::test]
    async fn auto_register_key_failure_returns_internal_error() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        pool.close().await;
        let cache: AuthCache = Arc::new(dashmap::DashMap::new());
        let result = auto_register_key(&pool, &cache, "newkey", 7).await;
        assert!(matches!(result, Err(AuthError::InternalError)));
        assert!(
            cache.get("newkey").is_none(),
            "failed insert must not cache"
        );
    }

    // Regression: a failed status lookup must propagate as InternalError, not
    // flatten to None (which would bypass the open-mode existing-project guard).
    #[tokio::test]
    async fn project_status_lookup_failure_returns_internal_error() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        pool.close().await;
        let result = project_status_checked(&pool, 1).await;
        assert!(matches!(result, Err(AuthError::InternalError)));
    }

    #[tokio::test]
    async fn project_status_absent_row_is_none() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let result = project_status_checked(&pool, 424_242).await;
        assert!(matches!(result, Ok(None)));
    }

    // Mirrors the archive -> denied-ingest -> unarchive flow: an Archived denial cached while the project was archived must be gone once it's unarchived, so a valid key passes without waiting out NEGATIVE_AUTH_CACHE_TTL.
    #[test]
    fn unarchive_clears_cached_archived_denial() {
        let positive: AuthCache = Arc::new(dashmap::DashMap::new());
        let negative = new_negative_cache();
        negative.insert(
            ("valid_key".to_owned(), 42),
            NegativeEntry {
                denial: Denial::Archived,
                inserted_at: Instant::now(),
            },
        );
        let entry = negative.get(&("valid_key".to_owned(), 42)).unwrap();
        assert!(matches!(entry.denial, Denial::Archived));
        assert!(cache_fresh(
            entry.inserted_at.elapsed(),
            NEGATIVE_AUTH_CACHE_TTL
        ));
        drop(entry);

        invalidate_project(&positive, &negative, 42);
        assert!(negative.get(&("valid_key".to_owned(), 42)).is_none());
    }
}
