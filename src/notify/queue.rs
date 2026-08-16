//! Durable queue for notifications that exhausted their in-process retry.

use crate::commercial::LicenseHandle;
use crate::config::Config;
use crate::db::DbPool;
use crate::notify::{NotificationEvent, NotifyRuntime};
use crate::queries;
use crate::queries::notify_queue::{QueuedDelivery, STATUS_FAILED, STATUS_PENDING};
use crate::util::crypto::SecretEncryptor;
use std::sync::Arc;

pub const INITIAL_BACKOFF_SECS: i64 = 30;
pub const MAX_BACKOFF_SECS: i64 = 3600;
/// Retry window, measured from when the item was first queued.
pub const RETRY_WINDOW_SECS: i64 = 24 * 3600;

const DRAIN_INTERVAL_SECS: u64 = 15;
const DRAIN_BATCH: i64 = 25;

/// 30s doubling to a 1h cap. `attempts` is the count already made.
pub fn backoff_secs(attempts: i64) -> i64 {
    if attempts <= 1 {
        return INITIAL_BACKOFF_SECS;
    }
    // Clamp before the cast: a huge attempts count would wrap into a small shift.
    let shift = (attempts - 1).min(32) as u32;
    INITIAL_BACKOFF_SECS
        .checked_shl(shift)
        .unwrap_or(MAX_BACKOFF_SECS)
        .min(MAX_BACKOFF_SECS)
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub struct DeliveryTarget {
    pub project_id: i64,
    pub integration_id: i64,
}

/// Persist a failed delivery. The INSERT needs `writer_pool`; the read pool is `query_only` on SQLite.
pub async fn enqueue_failed(
    pool: &DbPool,
    writer_pool: &DbPool,
    target: &DeliveryTarget,
    event: &NotificationEvent,
    error: &str,
) {
    let payload = match serde_json::to_string(event) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("notify: cannot serialise notification for the queue: {e}");
            return;
        }
    };
    let org_id = match queries::orgs::org_of_project(pool, target.project_id).await {
        Ok(Some(o)) => o,
        Ok(None) => {
            tracing::warn!(
                "notify: dropping failed delivery for unknown project {}",
                target.project_id
            );
            return;
        }
        Err(e) => {
            tracing::warn!("notify: cannot resolve org for the delivery queue: {e}");
            return;
        }
    };

    let now = now_secs();
    if let Err(e) = queries::notify_queue::enqueue(
        writer_pool,
        org_id,
        target.project_id,
        target.integration_id,
        &payload,
        error,
        now,
        now + INITIAL_BACKOFF_SECS,
    )
    .await
    {
        tracing::error!("notify: failed to queue an undelivered notification: {e}");
    }
}

/// Everything the drain and the replay handler need to make one more attempt.
#[derive(Clone)]
pub struct DrainCtx {
    pub pool: DbPool,
    pub writer_pool: DbPool,
    pub encryptor: Option<Arc<SecretEncryptor>>,
    pub config: Arc<Config>,
    pub license: LicenseHandle,
    pub runtime: NotifyRuntime,
}

/// A queued item resolved against the current state of its integration.
pub struct PreparedAttempt {
    pub integration: queries::Integration,
    pub secret: Option<String>,
    /// The project's own config; `None` falls back to the integration's default recipient.
    pub project_config: Option<String>,
    pub event: NotificationEvent,
}

/// Why an item could not be attempted at all.
enum NotAttempted {
    /// Integration gone, or the project moved org: the row is dropped.
    Gone,
    /// Kept, and retried on a later tick.
    Blocked(String),
    /// The payload no longer parses; nothing can replay it, so the row is dropped.
    Corrupt(String),
}

/// Resolve a queued row against current state, or say why it cannot be tried.
async fn prepare(ctx: &DrainCtx, item: &QueuedDelivery) -> Result<PreparedAttempt, NotAttempted> {
    let event: NotificationEvent = serde_json::from_str(&item.payload)
        .map_err(|e| NotAttempted::Corrupt(format!("stored payload no longer parses: {e}")))?;

    // A project that changed org must not keep delivering to the old org's endpoint.
    match queries::orgs::org_of_project(&ctx.pool, item.project_id).await {
        Ok(Some(org_id)) if org_id == item.org_id => {}
        Ok(_) => return Err(NotAttempted::Gone),
        Err(e) => {
            return Err(NotAttempted::Blocked(format!(
                "cannot resolve the project's org: {e}"
            )))
        }
    }

    let integration = match queries::integrations::get_integration(
        &ctx.pool,
        item.integration_id,
        Some(item.org_id),
    )
    .await
    {
        Ok(Some(i)) => i,
        Ok(None) => return Err(NotAttempted::Gone),
        Err(e) => {
            return Err(NotAttempted::Blocked(format!(
                "cannot load integration: {e}"
            )))
        }
    };

    if !crate::commercial::providers::may_deliver(&ctx.license, integration.kind) {
        return Err(NotAttempted::Blocked(
            "an active commercial license is required".into(),
        ));
    }

    // Blocked, not dropped: lifting the exclusion has to resume the backlog.
    match queries::integration_exclusions::is_excluded(
        &ctx.pool,
        item.integration_id,
        item.project_id,
    )
    .await
    {
        Ok(false) => {}
        Ok(true) => {
            return Err(NotAttempted::Blocked(
                "the project is excluded from this integration".into(),
            ))
        }
        Err(e) => {
            return Err(NotAttempted::Blocked(format!(
                "cannot check exclusions: {e}"
            )))
        }
    }

    let secret = match (&integration.secret, integration.encrypted, &ctx.encryptor) {
        (Some(s), true, Some(enc)) => enc.decrypt(s),
        (Some(s), false, _) => Some(s.clone()),
        _ => None,
    };

    // Only the recipient is re-read - re-gating on min_level/environment_filter would drop the delivery.
    let project_config = queries::integrations::get_project_integration(
        &ctx.pool,
        item.project_id,
        item.integration_id,
    )
    .await
    .map_err(|e| NotAttempted::Blocked(format!("cannot load the project's integration row: {e}")))?
    .and_then(|pi| pi.config);

    Ok(PreparedAttempt {
        integration,
        secret,
        project_config,
        event,
    })
}

/// Make one outbound attempt. Holds no permit of its own - the caller acquires it.
pub async fn send_attempt(ctx: &DrainCtx, attempt: &PreparedAttempt) -> Result<(), String> {
    let integration = &attempt.integration;
    if let crate::domain::IntegrationKind::Email = integration.kind {
        let Some(email_cfg) = ctx.config.email.as_ref() else {
            return Err("email is not configured ([email] section absent)".into());
        };
        return crate::providers::email::send(
            email_cfg,
            &ctx.config.server.web_base(),
            attempt.secret.as_deref(),
            integration.config.as_deref(),
            attempt.project_config.as_deref(),
            &attempt.event,
        )
        .await
        .map_err(|e| e.to_string());
    }

    let url = integration
        .url
        .as_deref()
        .filter(|u| !u.is_empty())
        .ok_or_else(|| "integration has no URL configured".to_string())?;

    let resolved = crate::util::ssrf::check_ssrf(url).await?;
    let client = ctx
        .runtime
        .client(&resolved)
        .map_err(|e| format!("cannot build pinned client: {e}"))?;

    crate::providers::dispatch(
        &ctx.license,
        &client,
        &integration.kind,
        url,
        attempt.secret.as_deref(),
        &attempt.event,
    )
    .await
    .map_err(|e| e.to_string())
}

/// One manual attempt at a queued item; leaves the row alone so a replay never burns a retry.
pub async fn replay_once(ctx: &DrainCtx, item: &QueuedDelivery) -> Result<(), String> {
    match prepare(ctx, item).await {
        Ok(attempt) => {
            // Replay's own bound, not the dispatch one: this runs in a request handler with no timeout.
            let _permit = ctx.runtime.replay_permit().await;
            send_attempt(ctx, &attempt).await
        }
        Err(NotAttempted::Gone) => {
            Err("the integration no longer exists, or the project has moved org".into())
        }
        Err(NotAttempted::Blocked(why) | NotAttempted::Corrupt(why)) => Err(why),
    }
}

/// Apply one attempt's outcome: delivered rows go, failed rows back off or park.
async fn record_outcome(
    ctx: &DrainCtx,
    item: &QueuedDelivery,
    now: i64,
    outcome: Result<(), String>,
) {
    let result = match outcome {
        Ok(()) => queries::notify_queue::delete(&ctx.writer_pool, item.id, None)
            .await
            .map(|_| ()),
        Err(error) => {
            let exhausted = now - item.created_at >= RETRY_WINDOW_SECS;
            let status = if exhausted {
                STATUS_FAILED
            } else {
                STATUS_PENDING
            };
            let next = if exhausted {
                now
            } else {
                now + backoff_secs(item.attempts + 1)
            };
            queries::notify_queue::record_attempt(
                &ctx.writer_pool,
                item.id,
                &error,
                now,
                next,
                status,
            )
            .await
            .map(|_| ())
        }
    };
    if let Err(e) = result {
        tracing::warn!("notify: cannot update queued delivery {}: {e}", item.id);
    }
}

/// Work one batch of due items concurrently. `clock` and `send` are injected for tests.
async fn drain_batch<C, S, Fut>(ctx: &DrainCtx, clock: C, send: S) -> usize
where
    C: Fn() -> i64 + Send + Sync + 'static,
    S: Fn(PreparedAttempt) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<(), String>> + Send,
{
    let items = match queries::notify_queue::due(&ctx.pool, clock(), DRAIN_BATCH).await {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!("notify: cannot read the delivery queue: {e}");
            return 0;
        }
    };

    let clock = Arc::new(clock);
    let send = Arc::new(send);
    let mut units = tokio::task::JoinSet::new();
    for item in items {
        let ctx = ctx.clone();
        let clock = clock.clone();
        let send = send.clone();
        units.spawn(async move {
            match prepare(&ctx, &item).await {
                Ok(attempt) => {
                    let _permit = ctx.runtime.permit().await;
                    let outcome = (*send)(attempt).await;
                    record_outcome(&ctx, &item, (*clock)(), outcome).await;
                    1
                }
                Err(reason @ (NotAttempted::Gone | NotAttempted::Corrupt(_))) => {
                    if let NotAttempted::Corrupt(why) = &reason {
                        tracing::warn!("notify: dropping queued delivery {}: {why}", item.id);
                    }
                    if let Err(e) =
                        queries::notify_queue::delete(&ctx.writer_pool, item.id, None).await
                    {
                        tracing::warn!(
                            "notify: cannot drop unreplayable delivery {}: {e}",
                            item.id
                        );
                    }
                    0
                }
                Err(NotAttempted::Blocked(reason)) => {
                    record_outcome(&ctx, &item, (*clock)(), Err(reason)).await;
                    0
                }
            }
        });
    }

    let mut attempted = 0;
    while let Some(unit) = units.join_next().await {
        match unit {
            Ok(n) => attempted += n,
            Err(e) => tracing::warn!("notify: a queued delivery ended abnormally: {e}"),
        }
    }
    attempted
}

/// Enforce both retention bounds.
async fn sweep(ctx: &DrainCtx, now: i64) {
    let cutoff = now - ctx.config.notifications.queue_retention_days * 86_400;
    if let Err(e) = queries::notify_queue::purge_failed_before(&ctx.writer_pool, cutoff).await {
        tracing::warn!("notify: queue retention sweep failed: {e}");
    }
    match queries::notify_queue::trim_per_integration(
        &ctx.writer_pool,
        ctx.config.notifications.queue_max_per_integration,
    )
    .await
    {
        // Warn, not debug: the trim drops undelivered notifications.
        Ok(n) if n > 0 => tracing::warn!(
            "notify: queue trim dropped {n} undelivered deliveries over the per-integration cap of {}",
            ctx.config.notifications.queue_max_per_integration
        ),
        Ok(_) => {}
        Err(e) => tracing::warn!("notify: queue trim failed: {e}"),
    }
}

/// Retry due deliveries on a fixed tick until cancelled.
pub fn spawn_notify_queue_drain(ctx: DrainCtx, cancel: tokio_util::sync::CancellationToken) {
    crate::background::supervise("notify_queue_drain", async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(DRAIN_INTERVAL_SECS));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticker.tick() => {
                    let sender = {
                        let ctx = ctx.clone();
                        move |attempt: PreparedAttempt| {
                            let ctx = ctx.clone();
                            async move { send_attempt(&ctx, &attempt).await }
                        }
                    };
                    let attempted = drain_batch(&ctx, now_secs, sender).await;
                    if attempted > 0 {
                        tracing::debug!("notify: retried {attempted} queued deliveries");
                    }
                    sweep(&ctx, now_secs()).await;
                }
            }
        }
        tracing::info!("notify queue drain exiting");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::notify_queue as q;

    type SendFuture =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>;

    #[test]
    fn backoff_doubles_from_30s_and_stops_at_an_hour() {
        assert_eq!(backoff_secs(0), 30, "never below the first interval");
        assert_eq!(backoff_secs(1), 30);
        assert_eq!(backoff_secs(2), 60);
        assert_eq!(backoff_secs(3), 120);
        assert_eq!(backoff_secs(6), 960);
        assert_eq!(backoff_secs(7), 1920);
        assert_eq!(backoff_secs(8), MAX_BACKOFF_SECS, "capped, not 3840");
        assert_eq!(backoff_secs(200), MAX_BACKOFF_SECS);
        assert_eq!(backoff_secs(i64::MAX), MAX_BACKOFF_SECS);
    }

    /// Counts network reaches instead of panicking - a panic in a spawned task is only logged.
    fn recording_sender() -> (
        Arc<std::sync::atomic::AtomicUsize>,
        impl Fn(PreparedAttempt) -> SendFuture + Send + Sync + 'static,
    ) {
        let reached = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = reached.clone();
        let sender = move |_: PreparedAttempt| {
            let counter = counter.clone();
            Box::pin(async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }) as SendFuture
        };
        (reached, sender)
    }

    fn event() -> NotificationEvent {
        NotificationEvent {
            trigger: crate::notify::NotifyTrigger::NewIssue,
            project_id: 42,
            fingerprint: "fp".into(),
            title: Some("boom".into()),
            level: Some("error".into()),
            environment: None,
            environments: Vec::new(),
            event_id: "e1".into(),
            digest: None,
        }
    }

    async fn ctx_with_project(pool: DbPool) -> DrainCtx {
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
        DrainCtx {
            writer_pool: pool.clone(),
            pool,
            encryptor: None,
            config: Arc::new(Config::default()),
            license: crate::commercial::fully_licensed(),
            runtime: NotifyRuntime::new(),
        }
    }

    async fn integration(ctx: &DrainCtx, name: &str, url: &str) -> i64 {
        queries::integrations::create_integration(
            &ctx.pool,
            5,
            name,
            "webhook",
            Some(url),
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn a_queued_delivery_is_retried_against_a_recovered_sink_and_then_dropped() {
        let pool = crate::db::open_test_pool().await;
        let ctx = ctx_with_project(pool.clone()).await;

        // Real sink: the production sender can't be pointed here, check_ssrf rejects loopback.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = Arc::new(parking_lot::Mutex::new(String::new()));
        {
            let hits = hits.clone();
            let seen = seen.clone();
            tokio::spawn(async move {
                loop {
                    let Ok((mut sock, _)) = listener.accept().await else {
                        return;
                    };
                    let hits = hits.clone();
                    let seen = seen.clone();
                    tokio::spawn(async move {
                        use tokio::io::{AsyncReadExt, AsyncWriteExt};
                        let mut buf = vec![0u8; 4096];
                        let n = sock.read(&mut buf).await.unwrap_or(0);
                        *seen.lock() = String::from_utf8_lossy(&buf[..n]).to_string();
                        hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        let _ = sock
                            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                            .await;
                    });
                }
            });
        }

        let id = integration(&ctx, "hooks", &format!("http://{addr}/hook")).await;
        let payload = serde_json::to_string(&event()).unwrap();
        q::enqueue(
            &ctx.pool,
            5,
            42,
            id,
            &payload,
            "connection refused",
            1000,
            1030,
        )
        .await
        .unwrap();

        let untouched = drain_batch(&ctx, || 1029, |_| async { Ok(()) }).await;
        assert_eq!(untouched, 0);
        assert_eq!(q::list_for_org(&ctx.pool, 5, 50).await.unwrap().len(), 1);

        let attempted = drain_batch(
            &ctx,
            || 1030,
            |_| async { Err("connection refused".to_string()) },
        )
        .await;
        assert_eq!(attempted, 1);
        let after = q::list_for_org(&ctx.pool, 5, 50).await.unwrap();
        assert_eq!(after[0].attempts, 2);
        assert_eq!(
            after[0].next_attempt_at,
            1030 + backoff_secs(2),
            "the next wait follows the attempt count this row has now reached"
        );
        assert_eq!(after[0].status, STATUS_PENDING);

        let client = reqwest::Client::new();
        let license = ctx.license.clone();
        let due_at = after[0].next_attempt_at;
        let attempted = drain_batch(
            &ctx,
            move || due_at,
            move |attempt| {
                let client = client.clone();
                let license = license.clone();
                async move {
                    crate::providers::dispatch(
                        &license,
                        &client,
                        &attempt.integration.kind,
                        attempt.integration.url.as_deref().unwrap(),
                        attempt.secret.as_deref(),
                        &attempt.event,
                    )
                    .await
                    .map_err(|e| e.to_string())
                }
            },
        )
        .await;
        assert_eq!(attempted, 1);
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(seen.lock().contains("POST /hook"));
        assert!(
            q::list_for_org(&ctx.pool, 5, 50).await.unwrap().is_empty(),
            "a delivered item leaves the queue"
        );
    }

    #[tokio::test]
    async fn an_item_past_the_retry_window_parks_as_failed() {
        let pool = crate::db::open_test_pool().await;
        let ctx = ctx_with_project(pool.clone()).await;
        let id = integration(&ctx, "hooks-dead", "https://dead.test/x").await;
        let payload = serde_json::to_string(&event()).unwrap();
        q::enqueue(&ctx.pool, 5, 42, id, &payload, "boom", 1000, 1000)
            .await
            .unwrap();

        let now = 1000 + RETRY_WINDOW_SECS;
        drain_batch(
            &ctx,
            move || now,
            |_| async { Err("still down".to_string()) },
        )
        .await;

        let items = q::list_for_org(&ctx.pool, 5, 50).await.unwrap();
        assert_eq!(items[0].status, STATUS_FAILED);
        assert_eq!(items[0].last_error.as_deref(), Some("still down"));
        assert!(
            q::due(&ctx.pool, now + 100_000, 10)
                .await
                .unwrap()
                .is_empty(),
            "a parked item is never picked up again by the drain"
        );
    }

    #[tokio::test]
    async fn a_corrupt_row_is_dropped_without_stopping_the_batch() {
        let pool = crate::db::open_test_pool().await;
        let ctx = ctx_with_project(pool.clone()).await;
        let id = integration(&ctx, "hooks-mixed", "https://ok.test/x").await;
        q::enqueue(&ctx.pool, 5, 42, id, "not json at all", "boom", 1000, 1000)
            .await
            .unwrap();
        let payload = serde_json::to_string(&event()).unwrap();
        q::enqueue(&ctx.pool, 5, 42, id, &payload, "boom", 1000, 1000)
            .await
            .unwrap();

        let attempted = drain_batch(&ctx, || 1000, |_| async { Ok(()) }).await;
        assert_eq!(attempted, 1, "the good row still went out");
        assert!(
            q::list_for_org(&ctx.pool, 5, 50).await.unwrap().is_empty(),
            "the corrupt row is dropped rather than retried forever"
        );
    }

    #[tokio::test]
    async fn a_lapsed_license_blocks_retries_without_discarding_the_item() {
        let pool = crate::db::open_test_pool().await;
        let mut ctx = ctx_with_project(pool.clone()).await;
        let id = integration(&ctx, "hooks-unlicensed", "https://ok.test/x").await;
        let payload = serde_json::to_string(&event()).unwrap();
        q::enqueue(&ctx.pool, 5, 42, id, &payload, "boom", 1000, 1000)
            .await
            .unwrap();

        ctx.license = crate::commercial::LicenseHandle::new(
            crate::commercial::license::LicenseStatus::Unlicensed,
            0,
        );
        let (reached, sender) = recording_sender();
        let attempted = drain_batch(&ctx, || 1000, sender).await;
        assert_eq!(attempted, 0);
        assert_eq!(
            reached.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "must not reach the network without a license"
        );

        let items = q::list_for_org(&ctx.pool, 5, 50).await.unwrap();
        assert_eq!(
            items.len(),
            1,
            "the item survives for when the license returns"
        );
        assert!(items[0]
            .last_error
            .as_deref()
            .is_some_and(|e| e.contains("license")));
    }

    async fn email_integration(ctx: &DrainCtx, name: &str) -> i64 {
        queries::integrations::create_integration(
            &ctx.pool, 5, name, "email", None, None, None, false, false,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn a_queued_email_waits_for_a_dispatch_permit_like_everything_else() {
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

        let pool = crate::db::open_test_pool().await;
        let ctx = ctx_with_project(pool.clone()).await;
        let id = email_integration(&ctx, "mail-permit").await;
        let payload = serde_json::to_string(&event()).unwrap();
        q::enqueue(&ctx.pool, 5, 42, id, &payload, "boom", 1000, 1000)
            .await
            .unwrap();

        let mut held = Vec::new();
        for _ in 0..crate::notify::MAX_CONCURRENT_DISPATCH {
            held.push(ctx.runtime.permit().await.expect("the semaphore is open"));
        }

        let sent = Arc::new(AtomicUsize::new(0));
        let drain = {
            let ctx = ctx.clone();
            let sent = sent.clone();
            tokio::spawn(async move {
                drain_batch(
                    &ctx,
                    || 1000,
                    move |_| {
                        let sent = sent.clone();
                        async move {
                            sent.fetch_add(1, SeqCst);
                            Ok(())
                        }
                    },
                )
                .await
            })
        };

        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert_eq!(
            sent.load(SeqCst),
            0,
            "an email delivery must queue behind the cap like every other kind"
        );

        held.clear();
        assert_eq!(drain.await.unwrap(), 1);
        assert_eq!(sent.load(SeqCst), 1, "and it goes once a permit frees up");
    }

    #[tokio::test]
    async fn a_replay_does_not_queue_behind_saturated_live_delivery() {
        let pool = crate::db::open_test_pool().await;
        let ctx = ctx_with_project(pool.clone()).await;
        // No `[email]` section on the default config, so the attempt fails without a network hop.
        let id = email_integration(&ctx, "mail-replay").await;
        let payload = serde_json::to_string(&event()).unwrap();
        q::enqueue(&ctx.pool, 5, 42, id, &payload, "boom", 1000, 1000)
            .await
            .unwrap();
        let item = q::list_for_org(&ctx.pool, 5, 50).await.unwrap().remove(0);

        let mut held = Vec::new();
        for _ in 0..crate::notify::MAX_CONCURRENT_DISPATCH {
            held.push(ctx.runtime.permit().await.expect("the semaphore is open"));
        }

        let result =
            tokio::time::timeout(std::time::Duration::from_secs(2), replay_once(&ctx, &item)).await;
        drop(held);
        assert!(
            result.is_ok(),
            "replay has its own bound; it must not wait on the dispatch semaphore"
        );
        assert!(result.unwrap().is_err(), "and it still reports the failure");
    }

    #[tokio::test]
    async fn a_project_that_changed_org_loses_its_queued_deliveries() {
        let pool = crate::db::open_test_pool().await;
        let ctx = ctx_with_project(pool.clone()).await;
        let id = integration(&ctx, "hooks-moved", "https://ok.test/x").await;
        let payload = serde_json::to_string(&event()).unwrap();
        q::enqueue(&ctx.pool, 5, 42, id, &payload, "boom", 1000, 1000)
            .await
            .unwrap();

        sqlx::query(crate::db::sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (6, 'other', 'Other')
             ON CONFLICT (org_id) DO NOTHING"
        ))
        .execute(&ctx.pool)
        .await
        .unwrap();
        sqlx::query(crate::db::sql!(
            "UPDATE projects SET org_id = 6 WHERE project_id = 42"
        ))
        .execute(&ctx.pool)
        .await
        .unwrap();

        let (reached, sender) = recording_sender();
        let attempted = drain_batch(&ctx, || 1000, sender).await;
        assert_eq!(attempted, 0);
        assert_eq!(
            reached.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "must not deliver a moved project's alert to its old org"
        );
        assert!(
            q::list_for_org(&ctx.pool, 5, 50).await.unwrap().is_empty(),
            "the row is dropped rather than left retrying against the old org"
        );
    }

    #[tokio::test]
    async fn an_exclusion_stops_the_backlog_but_does_not_discard_it() {
        let pool = crate::db::open_test_pool().await;
        let ctx = ctx_with_project(pool.clone()).await;
        let id = integration(&ctx, "hooks-excluded", "https://ok.test/x").await;
        let payload = serde_json::to_string(&event()).unwrap();
        q::enqueue(&ctx.pool, 5, 42, id, &payload, "boom", 1000, 1000)
            .await
            .unwrap();
        queries::integration_exclusions::exclude(&ctx.pool, 5, id, 42)
            .await
            .unwrap();

        let (reached, sender) = recording_sender();
        let attempted = drain_batch(&ctx, || 1000, sender).await;
        assert_eq!(attempted, 0);
        assert_eq!(
            reached.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "must not deliver to an excluded project"
        );

        let items = q::list_for_org(&ctx.pool, 5, 50).await.unwrap();
        assert_eq!(items.len(), 1, "the item survives the exclusion");
        assert!(items[0]
            .last_error
            .as_deref()
            .is_some_and(|e| e.contains("excluded")));
    }

    #[tokio::test]
    async fn lifting_an_exclusion_lets_the_backlog_through_again() {
        let pool = crate::db::open_test_pool().await;
        let ctx = ctx_with_project(pool.clone()).await;
        let id = integration(&ctx, "hooks-unexcluded", "https://ok.test/x").await;
        let payload = serde_json::to_string(&event()).unwrap();
        q::enqueue(&ctx.pool, 5, 42, id, &payload, "boom", 1000, 1000)
            .await
            .unwrap();
        queries::integration_exclusions::exclude(&ctx.pool, 5, id, 42)
            .await
            .unwrap();
        drain_batch(&ctx, || 1000, |_| async { Ok(()) }).await;

        queries::integration_exclusions::un_exclude(&ctx.pool, 5, id, 42)
            .await
            .unwrap();
        let attempted = drain_batch(&ctx, || 1000 + backoff_secs(2), |_| async { Ok(()) }).await;
        assert_eq!(attempted, 1, "the next tick resumes delivery on its own");
        assert!(q::list_for_org(&ctx.pool, 5, 50).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_failed_project_config_read_blocks_rather_than_using_the_default_recipient() {
        let pool = crate::db::open_test_pool().await;
        let ctx = ctx_with_project(pool.clone()).await;
        let id = integration(&ctx, "hooks-unreadable", "https://ok.test/x").await;
        let payload = serde_json::to_string(&event()).unwrap();
        q::enqueue(&ctx.pool, 5, 42, id, &payload, "boom", 1000, 1000)
            .await
            .unwrap();

        // Renamed back before any assertion, so a failure can't strand the shared Postgres DB.
        sqlx::query("ALTER TABLE project_integrations RENAME TO project_integrations_hidden")
            .execute(&ctx.pool)
            .await
            .unwrap();
        let attempted = drain_batch(&ctx, || 1000, |_| async { Ok(()) }).await;
        let items = q::list_for_org(&ctx.pool, 5, 50).await.unwrap();
        sqlx::query("ALTER TABLE project_integrations_hidden RENAME TO project_integrations")
            .execute(&ctx.pool)
            .await
            .unwrap();

        assert_eq!(attempted, 0, "an unreadable config must not be sent past");
        assert_eq!(items.len(), 1, "the item is kept for the next tick");
        assert!(items[0]
            .last_error
            .as_deref()
            .is_some_and(|e| e.contains("integration row")));
    }

    #[tokio::test]
    async fn each_item_backs_off_from_its_own_completion_not_the_batch_start() {
        use std::sync::atomic::{AtomicI64, Ordering::SeqCst};

        let pool = crate::db::open_test_pool().await;
        let ctx = ctx_with_project(pool.clone()).await;
        let id = integration(&ctx, "hooks-clock", "https://ok.test/x").await;
        let payload = serde_json::to_string(&event()).unwrap();
        for _ in 0..2 {
            q::enqueue(&ctx.pool, 5, 42, id, &payload, "boom", 1000, 1000)
                .await
                .unwrap();
        }

        // Reads 1000 to select, then 1100 and 1200 as the two items finish.
        let ticks = Arc::new(AtomicI64::new(1000));
        let clock = {
            let ticks = ticks.clone();
            move || ticks.fetch_add(100, SeqCst)
        };
        let attempted = drain_batch(&ctx, clock, |_| async { Err("down".to_string()) }).await;
        assert_eq!(attempted, 2);

        let mut nexts: Vec<i64> = q::list_for_org(&ctx.pool, 5, 50)
            .await
            .unwrap()
            .iter()
            .map(|i| i.next_attempt_at)
            .collect();
        nexts.sort_unstable();
        assert_eq!(
            nexts,
            vec![1100 + backoff_secs(2), 1200 + backoff_secs(2)],
            "each row's backoff runs from when that row finished"
        );
    }

    #[tokio::test]
    async fn a_batch_of_slow_items_costs_the_slowest_not_the_sum() {
        let pool = crate::db::open_test_pool().await;
        let ctx = ctx_with_project(pool.clone()).await;
        let id = integration(&ctx, "hooks-slow", "https://ok.test/x").await;
        let payload = serde_json::to_string(&event()).unwrap();
        for _ in 0..8 {
            q::enqueue(&ctx.pool, 5, 42, id, &payload, "boom", 1000, 1000)
                .await
                .unwrap();
        }

        let started = std::time::Instant::now();
        let attempted = drain_batch(
            &ctx,
            || 1000,
            |_| async {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                Ok(())
            },
        )
        .await;
        let elapsed = started.elapsed();

        assert_eq!(attempted, 8);
        assert!(
            elapsed < std::time::Duration::from_millis(800),
            "8 items at 200ms should finish near 200ms, not 1.6s; took {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn retention_drops_old_failed_items_and_caps_rows_per_integration() {
        let pool = crate::db::open_test_pool().await;
        let mut ctx = ctx_with_project(pool.clone()).await;
        let id = integration(&ctx, "hooks-retention", "https://ok.test/x").await;
        let payload = serde_json::to_string(&event()).unwrap();

        let now = 10_000_000;
        let old = now - 15 * 86_400;
        for _ in 0..4 {
            q::enqueue(&ctx.pool, 5, 42, id, &payload, "boom", old, old)
                .await
                .unwrap();
        }
        // Only failed items age out; a pending one is still inside its window.
        let all = q::list_for_org(&ctx.pool, 5, 50).await.unwrap();
        for item in all.iter().take(3) {
            q::record_attempt(&ctx.pool, item.id, "boom", old, old, STATUS_FAILED)
                .await
                .unwrap();
        }

        sweep(&ctx, now).await;
        let left = q::list_for_org(&ctx.pool, 5, 50).await.unwrap();
        assert_eq!(
            left.len(),
            1,
            "three failed items aged out, the pending one stayed"
        );
        assert_eq!(left[0].status, STATUS_PENDING);

        // Count bound, independent of age.
        for _ in 0..5 {
            q::enqueue(&ctx.pool, 5, 42, id, &payload, "boom", now, now)
                .await
                .unwrap();
        }
        let cfg = Config {
            notifications: crate::config::NotificationsConfig {
                queue_max_per_integration: 2,
                ..Default::default()
            },
            ..Default::default()
        };
        ctx.config = Arc::new(cfg);
        sweep(&ctx, now).await;

        let left = q::list_for_org(&ctx.pool, 5, 50).await.unwrap();
        assert_eq!(left.len(), 2, "oldest dropped first, newest two kept");
        assert!(left[0].id > left[1].id);
    }
}
