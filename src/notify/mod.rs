pub mod rate_limit;

use crate::crypto::SecretEncryptor;
use crate::db::DbPool;
use crate::providers;
use crate::queries;
use rate_limit::NotifyRateLimiter;
use std::sync::Arc;

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

/// Spawn the dispatcher under a panic-observing supervisor.
///
/// The dispatcher owns the mpsc Receiver, so if the future panics
/// the channel is dropped and all senders break — supervision here
/// logs the panic but cannot cleanly restart without rebuilding the
/// whole channel. Treat a panic as fatal for notifications; fix the
/// underlying bug and restart the process.
pub fn spawn_dispatcher(
    rx: tokio::sync::mpsc::Receiver<NotificationEvent>,
    pool: DbPool,
    encryptor: Option<Arc<SecretEncryptor>>,
    rate_limiter: Arc<NotifyRateLimiter>,
) {
    crate::background::supervise(
        "notify_dispatcher",
        run_dispatcher(rx, pool, encryptor, rate_limiter),
    );
}

pub async fn run_dispatcher(
    mut rx: tokio::sync::mpsc::Receiver<NotificationEvent>,
    pool: DbPool,
    encryptor: Option<Arc<SecretEncryptor>>,
    rate_limiter: Arc<NotifyRateLimiter>,
) {
    tracing::info!("notification dispatcher started");

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

            let kind = pi.integration_kind.clone();
            let url = pi.integration_url.clone();
            let int_config = pi.integration_config.clone();
            let pi_config = pi.config.clone();
            let name = pi.integration_name.clone();
            let event = event.clone();

            tokio::spawn(async move {
                // Resolve DNS and block webhooks pointing at private/internal addresses.
                let resolved = match crate::ssrf::check_ssrf(&url).await {
                    Ok(r) => r,
                    Err(msg) => {
                        tracing::warn!("notify: {name} blocked by SSRF check: {msg}");
                        return;
                    }
                };

                let pinned_client = match reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .redirect(reqwest::redirect::Policy::none())
                    .resolve(&resolved.hostname, resolved.addr)
                    .build()
                {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("notify: failed to build pinned client: {e}");
                        return;
                    }
                };

                let result = providers::dispatch(
                    &pinned_client,
                    &kind,
                    &url,
                    secret.as_deref(),
                    int_config.as_deref(),
                    pi_config.as_deref(),
                    &event,
                )
                .await;

                if let Err(e) = result {
                    tracing::warn!("notify: {name} ({kind}) failed, retrying: {e}");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    if let Err(e2) = providers::dispatch(
                        &pinned_client,
                        &kind,
                        &url,
                        secret.as_deref(),
                        int_config.as_deref(),
                        pi_config.as_deref(),
                        &event,
                    )
                    .await
                    {
                        tracing::error!("notify: {name} ({kind}) retry failed, dropping: {e2}");
                    }
                }
            });
        }
    }

    tracing::info!("notification dispatcher exiting");
}
