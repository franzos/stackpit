pub mod rate_limit;

use crate::commercial::LicenseHandle;
use crate::config::Config;
use crate::db::DbPool;
use crate::providers;
use crate::queries;
use crate::util::crypto::SecretEncryptor;
use lru::LruCache;
use parking_lot::Mutex;
use rate_limit::NotifyRateLimiter;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::Arc;

/// Reqwest clients keyed by (host, SSRF-resolved addr); each pins connections
/// to that addr so repeated deliveries reuse the pool and TLS.
/// LRU-bounded so rotating-DNS targets can't mint clients without limit.
type ClientCache = Arc<Mutex<LruCache<(String, SocketAddr), reqwest::Client>>>;

const CLIENT_CACHE_CAP: usize = 64;

#[derive(Debug, Clone)]
pub struct NotificationEvent {
    pub trigger: NotifyTrigger,
    pub project_id: u64,
    pub fingerprint: String,
    pub title: Option<String>,
    pub level: Option<String>,
    /// Representative environment for display (providers render this).
    pub environment: Option<String>,
    /// All environments this event spans; matched against `environment_filter`.
    pub environments: Vec<String>,
    pub event_id: String,
    pub digest: Option<DigestPayload>,
}

#[derive(Debug, Clone)]
pub enum NotifyTrigger {
    NewIssue,
    Regression,
    ThresholdExceeded {
        rule_id: i64,
        count: i64,
        window_secs: i64,
    },
    Digest,
}

impl NotifyTrigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            NotifyTrigger::NewIssue => "new_issue",
            NotifyTrigger::Regression => "regression",
            NotifyTrigger::ThresholdExceeded { .. } => "threshold_exceeded",
            NotifyTrigger::Digest => "digest",
        }
    }

    /// Human-facing label used in notification subjects/headers.
    pub fn display_label(&self) -> String {
        match self {
            NotifyTrigger::NewIssue => "New Issue".to_string(),
            NotifyTrigger::Regression => "Regression".to_string(),
            NotifyTrigger::ThresholdExceeded {
                count, window_secs, ..
            } => format!("Threshold: {count} events in {window_secs}s"),
            NotifyTrigger::Digest => "Digest".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DigestPayload {
    pub period_start: i64,
    pub period_end: i64,
    pub projects: Vec<DigestProject>,
    /// True when this is a preview built from example data (digest test with no
    /// real activity in the window); rendered with a "sample" banner.
    pub sample: bool,
}

#[derive(Debug, Clone)]
pub struct DigestProject {
    pub project_id: u64,
    pub name: Option<String>,
    pub new_issues: Vec<DigestIssue>,
    pub active_issues_count: u64,
    pub total_events: u64,
}

#[derive(Debug, Clone)]
pub struct DigestIssue {
    pub fingerprint: String,
    pub title: Option<String>,
    pub level: Option<String>,
    pub event_count: u64,
    pub first_seen: i64,
}

/// Cap on concurrent in-flight deliveries; each task holds a client and up to
/// ~22s of retry/timeout, so bursts queue rather than spawn unbounded tasks.
const MAX_CONCURRENT_DISPATCH: usize = 32;

/// Run `send`, retrying once after 2s on failure (drops on the second failure).
async fn send_with_one_retry<F, Fut, E>(name: &str, kind: &str, send: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<(), E>>,
    E: std::fmt::Display,
{
    if let Err(e) = send().await {
        tracing::warn!("notify: {name} ({kind}) failed, retrying: {e}");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if let Err(e2) = send().await {
            tracing::error!("notify: {name} ({kind}) retry failed, dropping: {e2}");
        }
    }
}

/// Fetch (or build and cache) a client pinned to `resolved.addr` for its host.
fn pinned_client(
    cache: &ClientCache,
    resolved: &crate::util::ssrf::ResolvedWebhook,
) -> anyhow::Result<reqwest::Client> {
    let key = (resolved.hostname.clone(), resolved.addr);
    if let Some(client) = cache.lock().get(&key) {
        return Ok(client.clone());
    }
    let client = crate::util::ssrf::build_pinned_client(resolved)?;
    cache.lock().put(key, client.clone());
    Ok(client)
}

fn passes_min_level(event_level: Option<&str>, min_level: Option<&str>) -> bool {
    match (event_level, min_level) {
        (_, None) => true,
        (None, Some(_)) => true, // event has no level: let it through rather than silently drop it
        (Some(ev), Some(min)) => {
            let ev_level: crate::ingest::models::Level =
                ev.parse().unwrap_or(crate::ingest::models::Level::Unknown);
            // A non-standard level string ranks lowest, which would suppress it;
            // Sentry treats an unparseable level as error, so let it through.
            if ev_level == crate::ingest::models::Level::Unknown {
                return true;
            }
            let min_level: crate::ingest::models::Level =
                min.parse().unwrap_or(crate::ingest::models::Level::Unknown);
            ev_level.rank() >= min_level.rank()
        }
    }
}

fn passes_env_filter(event_env: Option<&str>, filter: Option<&str>) -> bool {
    match (event_env, filter) {
        (_, None) | (_, Some("")) => true,
        (None, Some(_)) => false,
        (Some(ev), Some(f)) => ev == f,
    }
}

/// Env gate for aggregated events: a filter matches if it equals ANY environment
/// in the event's set. Falls back to the single-env check when the set is empty
/// (events that don't carry one, preserving prior behavior).
fn passes_env_set_filter(
    environments: &[String],
    event_env: Option<&str>,
    filter: Option<&str>,
) -> bool {
    match filter {
        None | Some("") => true,
        Some(f) => {
            if environments.is_empty() {
                passes_env_filter(event_env, Some(f))
            } else {
                environments.iter().any(|e| e == f)
            }
        }
    }
}

/// Spawn dispatcher with panic supervision (logs panics; restart needed for recovery).
pub fn spawn_dispatcher(
    rx: tokio::sync::mpsc::Receiver<NotificationEvent>,
    pool: DbPool,
    encryptor: Option<Arc<SecretEncryptor>>,
    config: Arc<Config>,
    rate_limiter: Arc<NotifyRateLimiter>,
    license: LicenseHandle,
) {
    crate::background::supervise(
        "notify_dispatcher",
        run_dispatcher(rx, pool, encryptor, config, rate_limiter, license),
    );
}

pub async fn run_dispatcher(
    mut rx: tokio::sync::mpsc::Receiver<NotificationEvent>,
    pool: DbPool,
    encryptor: Option<Arc<SecretEncryptor>>,
    config: Arc<Config>,
    rate_limiter: Arc<NotifyRateLimiter>,
    license: LicenseHandle,
) {
    tracing::info!("notification dispatcher started");

    let dispatch_limit = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_DISPATCH));
    let cap = NonZeroUsize::new(CLIENT_CACHE_CAP).expect("CLIENT_CACHE_CAP is non-zero");
    let client_cache: ClientCache = Arc::new(Mutex::new(LruCache::new(cap)));

    while let Some(event) = rx.recv().await {
        // Digests bypass rate limiting; they're already interval-controlled.
        if !matches!(event.trigger, NotifyTrigger::Digest) {
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            if !rate_limiter.check_and_record(event.project_id, now_secs) {
                tracing::warn!(
                    "notify: rate-limited notification for project {} (trigger={})",
                    event.project_id,
                    event.trigger.as_str(),
                );
                continue;
            }
        }

        let integrations =
            match queries::integrations::get_active_for_project(&pool, event.project_id).await {
                Ok(list) => list,
                Err(e) => {
                    tracing::warn!("notify: failed to query integrations: {e}");
                    continue;
                }
            };

        // Share one event across integrations rather than deep-cloning the
        // (potentially large) digest tree per task.
        let event = Arc::new(event);

        // Fire-and-forget so a slow webhook never blocks the next event.
        for pi in &integrations {
            match event.trigger {
                NotifyTrigger::NewIssue if !pi.notify_new_issues => continue,
                NotifyTrigger::Regression if !pi.notify_regressions => continue,
                NotifyTrigger::ThresholdExceeded { .. } if !pi.notify_threshold => continue,
                NotifyTrigger::Digest if !pi.notify_digests => continue,
                _ => {}
            }

            if !passes_min_level(event.level.as_deref(), pi.min_level.as_deref()) {
                continue;
            }

            if !passes_env_set_filter(
                &event.environments,
                event.environment.as_deref(),
                pi.environment_filter.as_deref(),
            ) {
                continue;
            }

            // Checked here as well as in `dispatch` so a lapsed license skips
            // the DNS/client work and doesn't get retried, and so the reason is
            // logged once per integration rather than dropped silently.
            if !crate::commercial::providers::may_deliver(&license, pi.integration_kind) {
                tracing::warn!(
                    "notify: skipping {} ({}) — an active commercial license is required",
                    pi.integration_name,
                    pi.integration_kind
                );
                continue;
            }

            let secret = match (&pi.integration_secret, pi.integration_encrypted, &encryptor) {
                (Some(s), true, Some(enc)) => enc.decrypt(s),
                (Some(s), false, _) => Some(s.clone()),
                _ => None,
            };

            let kind = pi.integration_kind;
            let url = pi.integration_url.clone();
            let int_config = pi.integration_config.clone();
            let pi_config = pi.config.clone();
            let name = pi.integration_name.clone();
            let event = Arc::clone(&event);
            let config = config.clone();
            let dispatch_limit = dispatch_limit.clone();
            let client_cache = client_cache.clone();
            let license = license.clone();

            tokio::spawn(async move {
                // Hold a permit for the task's lifetime so bursts queue rather
                // than spawn unbounded sends.
                let _permit = match dispatch_limit.acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => return,
                };

                let kind_label = kind.as_str();
                let web_base = config.server.web_base();

                // Email has no client/url/SSRF surface; polymail owns the endpoint.
                if let crate::domain::IntegrationKind::Email = kind {
                    let Some(email_cfg) = config.email.as_ref() else {
                        tracing::warn!(
                            "notify: {name} (email) skipped; no [email] section configured"
                        );
                        return;
                    };
                    send_with_one_retry(&name, kind_label, || {
                        providers::email::send(
                            email_cfg,
                            &web_base,
                            secret.as_deref(),
                            int_config.as_deref(),
                            pi_config.as_deref(),
                            &event,
                        )
                    })
                    .await;
                    return;
                }

                let url = match url {
                    Some(u) => u,
                    None => {
                        tracing::warn!("notify: {name} ({kind_label}) has no url; skipping");
                        return;
                    }
                };

                // Resolve DNS and block webhooks pointing at private/internal addresses.
                let resolved = match crate::util::ssrf::check_ssrf(&url).await {
                    Ok(r) => r,
                    Err(msg) => {
                        tracing::warn!("notify: {name} blocked by SSRF check: {msg}");
                        return;
                    }
                };

                let client = match pinned_client(&client_cache, &resolved) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("notify: failed to build pinned client: {e}");
                        return;
                    }
                };

                send_with_one_retry(&name, kind_label, || {
                    providers::dispatch(&license, &client, &kind, &url, secret.as_deref(), &event)
                })
                .await;
            });
        }
    }

    tracing::info!("notification dispatcher exiting");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_level_ranks_standard_levels() {
        assert!(passes_min_level(Some("error"), Some("warning")));
        assert!(!passes_min_level(Some("info"), Some("warning")));
        assert!(passes_min_level(Some("info"), None));
        assert!(passes_min_level(None, Some("fatal")));
    }

    // A non-standard SDK level ("critical") used to rank as debug and get
    // suppressed by every min_level filter.
    #[test]
    fn min_level_lets_unrecognized_levels_through() {
        assert!(passes_min_level(Some("critical"), Some("error")));
        assert!(passes_min_level(Some("err"), Some("fatal")));
    }

    #[test]
    fn env_set_filter_matches_any_environment() {
        // Single-env event: filter must match that env.
        assert!(passes_env_set_filter(
            &["production".into()],
            Some("production"),
            Some("production")
        ));
        assert!(!passes_env_set_filter(
            &["production".into()],
            Some("production"),
            Some("staging")
        ));

        // Multi-env delta: any-match against the filter.
        let envs = vec!["staging".to_string(), "production".to_string()];
        assert!(passes_env_set_filter(
            &envs,
            Some("staging"),
            Some("production")
        ));
        assert!(!passes_env_set_filter(&envs, Some("staging"), Some("qa")));
    }

    #[test]
    fn env_set_filter_no_filter_always_passes() {
        assert!(passes_env_set_filter(&["production".into()], None, None));
        assert!(passes_env_set_filter(
            &["production".into()],
            None,
            Some("")
        ));
        assert!(!passes_env_set_filter(&[], None, Some("production")));
    }

    #[test]
    fn env_set_filter_empty_set_falls_back_to_single_env() {
        // No set: behaves like passes_env_filter on the single env.
        assert!(passes_env_set_filter(
            &[],
            Some("production"),
            Some("production")
        ));
        assert!(!passes_env_set_filter(
            &[],
            Some("staging"),
            Some("production")
        ));
        // No env and no set, but a filter present: suppressed (prior behavior).
        assert!(!passes_env_set_filter(&[], None, Some("production")));
    }
}
