//! Project key validation with an in-memory cache in front of the DB.
//! Open mode auto-provisions a new project (with its first key) on an unknown
//! project_id; once a project exists, only registered keys are accepted.
//! Closed mode requires every project_id+key to exist.
//! HTTP-level handling (headers, responses) lives in `endpoints::auth`.

use crate::config::RegistrationMode;
use crate::queries;
use crate::queries::types::ProjectStatus;
use crate::server::AppState;
use std::sync::Arc;

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
