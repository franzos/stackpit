use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::db::DbPool;
use crate::util::stats::DiscardStats;

/// Log panics; do not restart (visibility over silent retry).
pub fn supervise<F>(name: &'static str, fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    let handle = tokio::spawn(fut);
    tokio::spawn(async move {
        match handle.await {
            Ok(()) => tracing::debug!("background task {name} exited cleanly"),
            Err(e) if e.is_panic() => {
                tracing::error!("background task {name} panicked — not restarted");
            }
            Err(e) if e.is_cancelled() => {
                tracing::debug!("background task {name} cancelled");
            }
            Err(e) => tracing::warn!("background task {name} join error: {e}"),
        }
    });
}

pub fn spawn_retention_task(pool: DbPool, retention_days: u32, cancel: CancellationToken) {
    if retention_days == 0 {
        return;
    }
    supervise("retention", async move {
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
            match crate::ingest::sourcemap::cleanup_stale_chunks(&pool, 86400).await {
                Ok(n) if n > 0 => tracing::info!("chunk cleanup: deleted {n} stale chunks"),
                Ok(_) => {}
                Err(e) => tracing::warn!("chunk cleanup error: {e}"),
            }
            // Same retention window as events.
            let sm_max_age = retention_days as i64 * 86400;
            match crate::ingest::sourcemap::cleanup_old_sourcemaps(&pool, sm_max_age).await {
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
    supervise("discard_stats", async move {
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

/// Hourly purge of expired OIDC grants, revocation markers, and JTI rows.
pub fn spawn_oidc_cleanup_task(pool: DbPool, cancel: CancellationToken) {
    supervise("oidc_cleanup", async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(3600)) => {}
            }
            let now = chrono::Utc::now().timestamp();
            match crate::oidc::grants::purge_expired(&pool, now).await {
                Ok(n) if n > 0 => tracing::info!("oidc cleanup: purged {n} expired grants"),
                Ok(_) => {}
                Err(e) => tracing::warn!("oidc grants purge error: {e}"),
            }
            match crate::oidc::revocations::purge_expired(&pool, now).await {
                Ok(n) if n > 0 => {
                    tracing::info!("oidc cleanup: purged {n} expired revocation/jti rows")
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("oidc revocations purge error: {e}"),
            }
        }
    });
}

pub fn spawn_wal_checkpoint_task(pool: DbPool, cancel: CancellationToken) {
    supervise("wal_checkpoint", async move {
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
    supervise("digest", async move {
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
            tracing::warn!("digest: failed to query schedules: {e}");
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
            schedule.org_id,
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

        if projects.is_empty() {
            // Advance last_sent so we don't keep re-checking the same empty period.
            if let Err(e) =
                crate::queries::alerts::update_digest_last_sent(pool, schedule.id, now).await
            {
                tracing::warn!("digest: failed to update last_sent: {e}");
            }
            continue;
        }

        let mut any_sent = false;
        if let Some(project_id) = schedule.project_id {
            let event = crate::notify::NotificationEvent {
                trigger: crate::notify::NotifyTrigger::Digest,
                project_id,
                fingerprint: String::new(),
                title: Some("Digest summary".to_string()),
                level: None,
                environment: None,
                environments: Vec::new(),
                event_id: String::new(),
                digest: Some(crate::notify::DigestPayload {
                    period_start,
                    period_end,
                    projects,
                    sample: false,
                }),
            };
            match notify_tx.try_send(event) {
                Ok(()) => any_sent = true,
                Err(e) => tracing::warn!("digest: dropped notification (channel full): {e}"),
            }
        } else {
            // Channels resolve per project (get_active_for_project), so each
            // event carries only its own project's data, never the whole org's.
            for project in projects {
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
                    environments: Vec::new(),
                    event_id: String::new(),
                    digest: Some(crate::notify::DigestPayload {
                        period_start,
                        period_end,
                        projects: vec![project],
                        sample: false,
                    }),
                };
                match notify_tx.try_send(event) {
                    Ok(()) => any_sent = true,
                    Err(e) => tracing::warn!("digest: dropped notification (channel full): {e}"),
                }
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

#[cfg(test)]
mod tests {
    use super::run_digest_cycle;
    use crate::queries::test_helpers::{insert_test_event, insert_test_issue, open_test_db};

    #[tokio::test]
    async fn global_digest_events_are_scoped_per_project() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO projects (project_id, name, status)
             VALUES (1, 'Project A', 'active'), (2, 'Project B', 'active')",
        )
        .execute(&pool)
        .await
        .unwrap();

        insert_test_event(
            &pool,
            "e1",
            1,
            now - 100,
            Some("fp-a"),
            Some("error"),
            Some("Error A"),
        )
        .await;
        insert_test_issue(
            &pool,
            "fp-a",
            1,
            Some("Error A"),
            Some("error"),
            now - 100,
            now - 100,
            1,
            "unresolved",
        )
        .await;
        insert_test_event(
            &pool,
            "e2",
            2,
            now - 100,
            Some("fp-b"),
            Some("error"),
            Some("Error B"),
        )
        .await;
        insert_test_issue(
            &pool,
            "fp-b",
            2,
            Some("Error B"),
            Some("error"),
            now - 100,
            now - 100,
            1,
            "unresolved",
        )
        .await;

        // Org-wide schedule (project_id = NULL), due immediately.
        crate::queries::alerts::create_digest_schedule(&pool, 1, None, 3600)
            .await
            .unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        run_digest_cycle(&pool, &tx).await;
        drop(tx);

        let mut events = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push(ev);
        }
        assert_eq!(events.len(), 2, "one event per project with activity");
        for ev in &events {
            let digest = ev.digest.as_ref().expect("digest payload present");
            assert_eq!(
                digest.projects.len(),
                1,
                "payload must contain only the event's own project"
            );
            assert_eq!(digest.projects[0].project_id, ev.project_id);
        }
    }
}
