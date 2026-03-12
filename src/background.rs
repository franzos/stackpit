use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::db::DbPool;
use crate::stats::DiscardStats;

pub fn spawn_retention_task(pool: DbPool, retention_days: u32, cancel: CancellationToken) {
    if retention_days == 0 {
        return;
    }
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(3600)) => {}
            }
            match crate::queries::retention::delete_old_events(&pool, retention_days).await {
                Ok(n) if n > 0 => tracing::info!("retention cleanup: deleted {n} old events"),
                Ok(_) => {}
                Err(e) => tracing::warn!("retention cleanup error: {e}"),
            }
            // Clean up stale upload chunks (older than 24h)
            match crate::sourcemap::cleanup_stale_chunks(&pool, 86400).await {
                Ok(n) if n > 0 => tracing::info!("chunk cleanup: deleted {n} stale chunks"),
                Ok(_) => {}
                Err(e) => tracing::warn!("chunk cleanup error: {e}"),
            }
            // Clean up old sourcemaps (same retention window as events)
            let sm_max_age = retention_days as i64 * 86400;
            match crate::sourcemap::cleanup_old_sourcemaps(&pool, sm_max_age).await {
                Ok(n) if n > 0 => tracing::info!("sourcemap cleanup: deleted {n} old sourcemaps"),
                Ok(_) => {}
                Err(e) => tracing::warn!("sourcemap cleanup error: {e}"),
            }
        }
    });
}

pub fn spawn_discard_stats_task(
    pool: DbPool,
    discard_stats: Arc<DiscardStats>,
    cancel: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(30)) => {}
            }
            if let Err(e) = discard_stats.flush(&pool).await {
                tracing::warn!("discard stats flush error: {e}");
            }
        }
    });
}

pub fn spawn_wal_checkpoint_task(pool: DbPool, cancel: CancellationToken) {
    tokio::spawn(async move {
        let _pool = pool;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(60)) => {}
            }
            #[cfg(feature = "sqlite")]
            if let Err(e) = crate::db::sqlite_pragma(&_pool, "PRAGMA wal_checkpoint(PASSIVE)").await
            {
                tracing::warn!("WAL checkpoint error: {e}");
            }
        }
    });
}

pub fn spawn_digest_task(
    pool: DbPool,
    notify_tx: tokio::sync::mpsc::Sender<crate::notify::NotificationEvent>,
    cancel: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(60)) => {}
            }
            run_digest_cycle(&pool, &notify_tx).await;
        }
    });
}

async fn run_digest_cycle(
    pool: &DbPool,
    notify_tx: &tokio::sync::mpsc::Sender<crate::notify::NotificationEvent>,
) {
    let now = chrono::Utc::now().timestamp();

    let schedules = match crate::queries::alerts::list_due_digests(pool, now).await {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!("digest: failed to query schedules: {e}");
            return;
        }
    };

    for schedule in &schedules {
        let period_start = schedule.last_sent;
        let period_end = now;

        let projects = match crate::queries::alerts::build_digest_data(
            pool,
            period_start,
            period_end,
            schedule.project_id,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    "digest: failed to build data for schedule {}: {e}",
                    schedule.id
                );
                continue;
            }
        };

        // Only send if there's something to report
        if projects.is_empty() {
            // Still update last_sent so we don't keep checking the same empty period
            if let Err(e) =
                crate::queries::alerts::update_digest_last_sent(pool, schedule.id, now).await
            {
                tracing::warn!("digest: failed to update last_sent: {e}");
            }
            continue;
        }

        let payload = crate::notify::DigestPayload {
            period_start,
            period_end,
            projects: projects.clone(),
        };

        // For digest notifications, we need to send to all project integrations
        // that have digests enabled. Use project_id 0 as a sentinel for global digests.
        let project_id = schedule.project_id.unwrap_or(0);

        // For global digests, send one notification per project that has activity
        let mut any_sent = false;
        if schedule.project_id.is_none() {
            for project in &projects {
                let event = crate::notify::NotificationEvent {
                    trigger: crate::notify::NotifyTrigger::Digest,
                    project_id: project.project_id,
                    fingerprint: String::new(),
                    title: Some(format!(
                        "Digest: {} new issues, {} events",
                        project.new_issues.len(),
                        project.total_events
                    )),
                    level: None,
                    environment: None,
                    event_id: String::new(),
                    digest: Some(payload.clone()),
                };
                match notify_tx.try_send(event) {
                    Ok(()) => any_sent = true,
                    Err(e) => tracing::warn!("digest: dropped notification (channel full): {e}"),
                }
            }
        } else {
            let event = crate::notify::NotificationEvent {
                trigger: crate::notify::NotifyTrigger::Digest,
                project_id,
                fingerprint: String::new(),
                title: Some("Digest summary".to_string()),
                level: None,
                environment: None,
                event_id: String::new(),
                digest: Some(payload),
            };
            match notify_tx.try_send(event) {
                Ok(()) => any_sent = true,
                Err(e) => tracing::warn!("digest: dropped notification (channel full): {e}"),
            }
        }

        if any_sent {
            if let Err(e) =
                crate::queries::alerts::update_digest_last_sent(pool, schedule.id, now).await
            {
                tracing::warn!("digest: failed to update last_sent: {e}");
            }
        }
    }
}
