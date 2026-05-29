pub mod rate_limit;

use crate::config::Config;
use crate::crypto::SecretEncryptor;
use crate::db::DbPool;
use crate::providers;
use crate::queries;
use dashmap::DashMap;
use rate_limit::NotifyRateLimiter;
use std::net::SocketAddr;
use std::sync::Arc;

/// Reqwest clients keyed by (host, SSRF-resolved addr). Each client pins its
/// connections to exactly that addr; repeated deliveries reuse the pool + TLS.
type ClientCache = Arc<DashMap<(String, SocketAddr), reqwest::Client>>;

#[derive(Debug, Clone)]
pub struct NotificationEvent {
    pub trigger: NotifyTrigger,
    pub project_id: u64,
    pub fingerprint: String,
    pub title: Option<String>,
    pub level: Option<String>,
    pub environment: Option<String>,
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

/// Cap on concurrent in-flight webhook/email deliveries. An alert burst can
/// match many integrations at once; each task holds a client and up to ~22s of
/// retry/timeout, so we queue past this rather than spawn unbounded tasks.
const MAX_CONCURRENT_DISPATCH: usize = 32;

/// Run `send`, and on error warn, wait 2s, then try once more (error+drop on
/// second failure). `name`/`kind` are only used for log context.
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

/// Fetch a cached client pinned to `resolved`, building (and caching) one on
/// first use. The pin stays exact: a client for a given (host, addr) only ever
/// resolves that host to that addr.
fn pinned_client(
    cache: &ClientCache,
    resolved: &crate::ssrf::ResolvedWebhook,
) -> Result<reqwest::Client, reqwest::Error> {
    let key = (resolved.hostname.clone(), resolved.addr);
    if let Some(client) = cache.get(&key) {
        return Ok(client.clone());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&resolved.hostname, resolved.addr)
        .build()?;
    cache.insert(key, client.clone());
    Ok(client)
}

fn passes_min_level(event_level: Option<&str>, min_level: Option<&str>) -> bool {
    match (event_level, min_level) {
        (_, None) => true,
        (None, Some(_)) => true, // no level on the event -- let it through rather than silently drop it
        (Some(ev), Some(min)) => {
            let ev_level: crate::models::Level =
                ev.parse().unwrap_or(crate::models::Level::Unknown);
            let min_level: crate::models::Level =
                min.parse().unwrap_or(crate::models::Level::Unknown);
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

/// Spawn dispatcher with panic supervision (logs panics; restart needed for recovery).
pub fn spawn_dispatcher(
    rx: tokio::sync::mpsc::Receiver<NotificationEvent>,
    pool: DbPool,
    encryptor: Option<Arc<SecretEncryptor>>,
    config: Arc<Config>,
    rate_limiter: Arc<NotifyRateLimiter>,
) {
    crate::background::supervise(
        "notify_dispatcher",
        run_dispatcher(rx, pool, encryptor, config, rate_limiter),
    );
}

pub async fn run_dispatcher(
    mut rx: tokio::sync::mpsc::Receiver<NotificationEvent>,
    pool: DbPool,
    encryptor: Option<Arc<SecretEncryptor>>,
    config: Arc<Config>,
    rate_limiter: Arc<NotifyRateLimiter>,
) {
    tracing::info!("notification dispatcher started");

    // Shared across every spawned delivery task to bound concurrent in-flight sends.
    let dispatch_limit = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_DISPATCH));

    // Reused across notifications so repeat deliveries to the same webhook
    // keep their connection pool and pinned-addr resolution.
    let client_cache: ClientCache = Arc::new(DashMap::new());

    while let Some(event) = rx.recv().await {
        // Digest notifications bypass rate limiting -- they're already interval-controlled
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

        // Share one event across all matching integrations rather than deep-cloning
        // the (potentially large) digest tree per task.
        let event = Arc::new(event);

        // Dispatch to all matching integrations concurrently.
        // Tasks are fire-and-forget so a slow webhook never blocks the
        // dispatcher from processing the next notification event.
        for pi in &integrations {
            // Skip if this integration doesn't care about this trigger
            match event.trigger {
                NotifyTrigger::NewIssue if !pi.notify_new_issues => continue,
                NotifyTrigger::Regression if !pi.notify_regressions => continue,
                NotifyTrigger::ThresholdExceeded { .. } if !pi.notify_threshold => continue,
                NotifyTrigger::Digest if !pi.notify_digests => continue,
                _ => {}
            }

            // Check severity threshold
            if !passes_min_level(event.level.as_deref(), pi.min_level.as_deref()) {
                continue;
            }

            // Check environment filter
            if !passes_env_filter(
                event.environment.as_deref(),
                pi.environment_filter.as_deref(),
            ) {
                continue;
            }

            // Decrypt the secret if it's encrypted, otherwise use as-is
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

            tokio::spawn(async move {
                // Hold a permit for the task's lifetime so a burst queues rather
                // than spawning unbounded concurrent sends. Closes only on shutdown.
                let _permit = match dispatch_limit.acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => return,
                };

                let kind_label = kind.as_str();

                // Email has no client/url/SSRF surface -- polymail owns the endpoint.
                if let crate::queries::types::IntegrationKind::Email = kind {
                    send_with_one_retry(&name, kind_label, || {
                        providers::email::send(
                            &config.email,
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
                let resolved = match crate::ssrf::check_ssrf(&url).await {
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
                    providers::dispatch(&client, &kind, &url, secret.as_deref(), &event)
                })
                .await;
            });
        }
    }

    tracing::info!("notification dispatcher exiting");
}
