pub mod queue;
pub mod rate_limit;

use crate::commercial::LicenseHandle;
use crate::config::Config;
use crate::db::DbPool;
use crate::providers;
use crate::queries;
use crate::util::crypto::SecretEncryptor;
use lru::LruCache;
use parking_lot::Mutex;
use queue::DeliveryTarget;
use rate_limit::NotifyRateLimiter;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::Arc;

/// Reqwest clients keyed by (host, SSRF-resolved addr); each pins connections
/// to that addr so repeated deliveries reuse the pool and TLS.
/// LRU-bounded so rotating-DNS targets can't mint clients without limit.
type ClientCache = Arc<Mutex<LruCache<(String, SocketAddr), reqwest::Client>>>;

const CLIENT_CACHE_CAP: usize = 64;

/// Persisted as JSON in the delivery queue, so new optional fields need `#[serde(default)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    pub trigger: NotifyTrigger,
    pub project_id: u64,
    pub fingerprint: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    /// Representative environment for display (providers render this).
    #[serde(default)]
    pub environment: Option<String>,
    /// All environments this event spans; matched against `environment_filter`.
    #[serde(default)]
    pub environments: Vec<String>,
    pub event_id: String,
    #[serde(default)]
    pub digest: Option<DigestPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestPayload {
    pub period_start: i64,
    pub period_end: i64,
    #[serde(default)]
    pub projects: Vec<DigestProject>,
    /// True when this is a preview built from example data (digest test with no
    /// real activity in the window); rendered with a "sample" banner.
    #[serde(default)]
    pub sample: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestProject {
    pub project_id: u64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub new_issues: Vec<DigestIssue>,
    pub active_issues_count: u64,
    pub total_events: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestIssue {
    pub fingerprint: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    pub event_count: u64,
    pub first_seen: i64,
}

/// Cap on concurrent in-flight deliveries; each task holds a client and up to
/// ~22s of retry/timeout, so bursts queue rather than spawn unbounded tasks.
const MAX_CONCURRENT_DISPATCH: usize = 32;

/// Separate from the dispatch bound so a manual replay can't starve automatic delivery.
const MAX_CONCURRENT_REPLAY: usize = 4;

/// Dispatch bounds and the pinned-client cache, shared by live sends and queue replays.
#[derive(Clone)]
pub struct NotifyRuntime {
    dispatch_limit: Arc<tokio::sync::Semaphore>,
    replay_limit: Arc<tokio::sync::Semaphore>,
    client_cache: ClientCache,
}

impl Default for NotifyRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl NotifyRuntime {
    pub fn new() -> Self {
        let cap = NonZeroUsize::new(CLIENT_CACHE_CAP).expect("CLIENT_CACHE_CAP is non-zero");
        Self {
            dispatch_limit: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_DISPATCH)),
            replay_limit: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_REPLAY)),
            client_cache: Arc::new(Mutex::new(LruCache::new(cap))),
        }
    }

    /// Held for a delivery's lifetime. `None` once the semaphore closes at shutdown.
    pub async fn permit(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.dispatch_limit.clone().acquire_owned().await.ok()
    }

    pub async fn replay_permit(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.replay_limit.clone().acquire_owned().await.ok()
    }

    /// Fetch (or build and cache) a client pinned to `resolved.addr` for its host.
    pub fn client(
        &self,
        resolved: &crate::util::ssrf::ResolvedWebhook,
    ) -> anyhow::Result<reqwest::Client> {
        let key = (resolved.hostname.clone(), resolved.addr);
        if let Some(client) = self.client_cache.lock().get(&key) {
            return Ok(client.clone());
        }
        let client = crate::util::ssrf::build_pinned_client(resolved)?;
        self.client_cache.lock().put(key, client.clone());
        Ok(client)
    }
}

/// Run `send`, retrying once after 2s, then queueing the notification.
#[allow(clippy::too_many_arguments)]
async fn send_or_enqueue<F, Fut, E>(
    pool: &DbPool,
    writer_pool: &DbPool,
    target: &DeliveryTarget,
    event: &NotificationEvent,
    name: &str,
    kind: &str,
    send: F,
) where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<(), E>>,
    E: std::fmt::Display,
{
    if let Err(e) = send().await {
        tracing::warn!("notify: {name} ({kind}) failed, retrying: {e}");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if let Err(e2) = send().await {
            tracing::warn!("notify: {name} ({kind}) retry failed, queueing: {e2}");
            queue::enqueue_failed(pool, writer_pool, target, event, &e2.to_string()).await;
        }
    }
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
#[allow(clippy::too_many_arguments)]
pub fn spawn_dispatcher(
    rx: tokio::sync::mpsc::Receiver<NotificationEvent>,
    pool: DbPool,
    writer_pool: DbPool,
    encryptor: Option<Arc<SecretEncryptor>>,
    config: Arc<Config>,
    rate_limiter: Arc<NotifyRateLimiter>,
    license: LicenseHandle,
    runtime: NotifyRuntime,
) {
    crate::background::supervise(
        "notify_dispatcher",
        run_dispatcher(
            rx,
            pool,
            writer_pool,
            encryptor,
            config,
            rate_limiter,
            license,
            runtime,
        ),
    );
}

/// `writer_pool` must be the write pool - SQLite sets `query_only=ON` on the reader.
#[allow(clippy::too_many_arguments)]
pub async fn run_dispatcher(
    mut rx: tokio::sync::mpsc::Receiver<NotificationEvent>,
    pool: DbPool,
    writer_pool: DbPool,
    encryptor: Option<Arc<SecretEncryptor>>,
    config: Arc<Config>,
    rate_limiter: Arc<NotifyRateLimiter>,
    license: LicenseHandle,
    runtime: NotifyRuntime,
) {
    tracing::info!("notification dispatcher started");

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
            let runtime = runtime.clone();
            let license = license.clone();
            let pool = pool.clone();
            let writer_pool = writer_pool.clone();
            let target = DeliveryTarget {
                project_id: pi.project_id as i64,
                integration_id: pi.integration_id,
            };

            tokio::spawn(async move {
                // Hold a permit for the task's lifetime so bursts queue rather
                // than spawn unbounded sends.
                let Some(_permit) = runtime.permit().await else {
                    return;
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
                    send_or_enqueue(
                        &pool,
                        &writer_pool,
                        &target,
                        &event,
                        &name,
                        kind_label,
                        || {
                            providers::email::send(
                                email_cfg,
                                &web_base,
                                secret.as_deref(),
                                int_config.as_deref(),
                                pi_config.as_deref(),
                                &event,
                            )
                        },
                    )
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

                let client = match runtime.client(&resolved) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("notify: failed to build pinned client: {e}");
                        return;
                    }
                };

                send_or_enqueue(
                    &pool,
                    &writer_pool,
                    &target,
                    &event,
                    &name,
                    kind_label,
                    || {
                        providers::dispatch(
                            &license,
                            &client,
                            &kind,
                            &url,
                            secret.as_deref(),
                            &event,
                        )
                    },
                )
                .await;
            });
        }
    }

    tracing::info!("notification dispatcher exiting");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> NotificationEvent {
        NotificationEvent {
            trigger: NotifyTrigger::ThresholdExceeded {
                rule_id: 7,
                count: 12,
                window_secs: 300,
            },
            project_id: 42,
            fingerprint: "fp".into(),
            title: Some("boom".into()),
            level: Some("error".into()),
            environment: Some("production".into()),
            environments: vec!["production".into(), "staging".into()],
            event_id: "abc".into(),
            digest: Some(DigestPayload {
                period_start: 1,
                period_end: 2,
                sample: false,
                projects: vec![DigestProject {
                    project_id: 42,
                    name: Some("web".into()),
                    active_issues_count: 3,
                    total_events: 9,
                    new_issues: vec![DigestIssue {
                        fingerprint: "fp2".into(),
                        title: None,
                        level: Some("warning".into()),
                        event_count: 4,
                        first_seen: 5,
                    }],
                }],
            }),
        }
    }

    #[test]
    fn the_event_family_round_trips_through_json() {
        let event = sample_event();
        let json = serde_json::to_string(&event).expect("serialises");
        let back: NotificationEvent = serde_json::from_str(&json).expect("deserialises");

        assert_eq!(back.project_id, 42);
        assert_eq!(back.environments, event.environments);
        assert_eq!(back.trigger.as_str(), "threshold_exceeded");
        assert_eq!(back.trigger.display_label(), event.trigger.display_label());
        let digest = back.digest.expect("digest survives");
        assert_eq!(digest.projects.len(), 1);
        assert_eq!(digest.projects[0].new_issues[0].fingerprint, "fp2");
        assert_eq!(digest.projects[0].new_issues[0].title, None);
    }

    /// `drain_batch` deletes rows it can't parse, so a missing default wipes the backlog on upgrade.
    #[test]
    fn a_payload_missing_every_optional_field_still_parses() {
        let json = r#"{
            "trigger": "NewIssue",
            "project_id": 42,
            "fingerprint": "fp",
            "event_id": "abc"
        }"#;
        let event: NotificationEvent =
            serde_json::from_str(json).expect("older payload still reads");
        assert_eq!(event.project_id, 42);
        assert_eq!(event.title, None);
        assert!(event.environments.is_empty());
        assert!(event.digest.is_none());

        let digest_json = r#"{
            "period_start": 1,
            "period_end": 2,
            "projects": [{"project_id": 42, "active_issues_count": 3, "total_events": 9}]
        }"#;
        let digest: DigestPayload = serde_json::from_str(digest_json).expect("older digest reads");
        assert!(!digest.sample);
        assert_eq!(digest.projects[0].name, None);
        assert!(digest.projects[0].new_issues.is_empty());
    }

    async fn queue_fixture() -> (DbPool, DeliveryTarget) {
        let pool = crate::db::open_test_pool().await;
        sqlx::query(crate::db::sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (5, 'acme', 'Acme')
             ON CONFLICT (org_id) DO NOTHING"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(crate::db::sql!(
            "INSERT INTO projects (project_id, name, org_id) VALUES (42, 'web', 5)"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let integration_id = queries::integrations::create_integration(
            &pool,
            5,
            "hooks",
            "webhook",
            Some("https://hooks.test/x"),
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap();
        (
            pool,
            DeliveryTarget {
                project_id: 42,
                integration_id,
            },
        )
    }

    #[tokio::test]
    async fn a_send_that_fails_twice_is_queued_with_its_payload_intact() {
        let (pool, target) = queue_fixture().await;
        let event = sample_event();

        // Real time: a paused clock auto-advances the pool's acquire timeout into a failure.
        send_or_enqueue(
            &pool,
            &pool,
            &target,
            &event,
            "hooks",
            "webhook",
            || async { Err::<(), _>(anyhow::anyhow!("connection refused")) },
        )
        .await;

        let items = queries::notify_queue::list_for_org(&pool, 5, 50)
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].project_id, 42);
        assert_eq!(items[0].integration_id, target.integration_id);
        assert_eq!(items[0].last_error.as_deref(), Some("connection refused"));

        let restored: NotificationEvent = serde_json::from_str(&items[0].payload).unwrap();
        assert_eq!(restored.event_id, "abc");
        assert_eq!(restored.digest.unwrap().projects[0].total_events, 9);
    }

    /// Needs real file-backed pools - `open_test_pool` hands out a writer and would pass regardless.
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[tokio::test]
    async fn a_failed_delivery_is_queued_through_the_writer_pool_not_the_read_pool() {
        let path = std::env::temp_dir().join(format!(
            "stackpit-notify-queue-pools-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let url = format!("sqlite://{}?mode=rwc", path.display());

        let writer_pool = crate::db::create_writer_pool(&url).await.unwrap();
        crate::db::run_migrations(&writer_pool).await.unwrap();
        let pool = crate::db::create_pool(&url).await.unwrap();

        sqlx::query("INSERT INTO organizations (org_id, slug, name) VALUES (5, 'acme', 'Acme')")
            .execute(&writer_pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects (project_id, name, org_id) VALUES (42, 'web', 5)")
            .execute(&writer_pool)
            .await
            .unwrap();
        let integration_id = queries::integrations::create_integration(
            &writer_pool,
            5,
            "hooks",
            "webhook",
            Some("https://hooks.test/x"),
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap();
        let target = DeliveryTarget {
            project_id: 42,
            integration_id,
        };

        assert!(
            sqlx::query("INSERT INTO organizations (org_id, slug, name) VALUES (6, 'x', 'X')")
                .execute(&pool)
                .await
                .is_err(),
            "the read pool must stay query_only; relaxing it is not the fix"
        );

        send_or_enqueue(
            &pool,
            &writer_pool,
            &target,
            &sample_event(),
            "hooks",
            "webhook",
            || async { Err::<(), _>(anyhow::anyhow!("connection refused")) },
        )
        .await;

        let items = queries::notify_queue::list_for_org(&pool, 5, 50)
            .await
            .unwrap();
        assert_eq!(
            items.len(),
            1,
            "the delivery must land in the queue even though the dispatcher's own pool cannot write"
        );
        assert_eq!(items[0].integration_id, integration_id);

        drop(pool);
        drop(writer_pool);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn a_send_that_succeeds_on_the_retry_is_not_queued() {
        let (pool, target) = queue_fixture().await;
        let event = sample_event();
        let attempts = std::sync::atomic::AtomicUsize::new(0);

        tokio::time::pause();
        send_or_enqueue(
            &pool,
            &pool,
            &target,
            &event,
            "hooks",
            "webhook",
            || async {
                if attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    Err(anyhow::anyhow!("transient"))
                } else {
                    Ok(())
                }
            },
        )
        .await;
        tokio::time::resume();

        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(
            queries::notify_queue::list_for_org(&pool, 5, 50)
                .await
                .unwrap()
                .is_empty(),
            "the in-process retry still resolves a blip without touching the queue"
        );
    }

    #[tokio::test]
    async fn a_successful_send_leaves_the_queue_empty() {
        let (pool, target) = queue_fixture().await;
        send_or_enqueue(
            &pool,
            &pool,
            &target,
            &sample_event(),
            "hooks",
            "webhook",
            || async { Ok::<(), anyhow::Error>(()) },
        )
        .await;
        assert!(queries::notify_queue::list_for_org(&pool, 5, 50)
            .await
            .unwrap()
            .is_empty());
    }

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
