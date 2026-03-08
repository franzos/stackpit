use anyhow::Result;
use sqlx::Row;

use crate::db::{sql, DbPool};

/// Delete old events in bounded chunks so no single transaction holds the
/// write lock for more than ~1 second.  Each chunk deletes up to
/// `CHUNK_LIMIT` rows, reconciles the affected issues, then commits --
/// giving the main writer a chance to acquire the lock between rounds.
const CHUNK_LIMIT: i64 = 5000;

pub async fn delete_old_events(pool: &DbPool, retention_days: u32) -> Result<usize> {
    let cutoff = chrono::Utc::now().timestamp() - (retention_days as i64 * 86400);
    let mut total_deleted: usize = 0;

    loop {
        let mut tx = pool.begin().await?;

        #[cfg(feature = "sqlite")]
        let delete_sql = sql!(
            "DELETE FROM events WHERE rowid IN (
                SELECT rowid FROM events WHERE received_at < ?1 LIMIT ?2
            )"
        );
        #[cfg(not(feature = "sqlite"))]
        let delete_sql = sql!(
            "DELETE FROM events WHERE ctid IN (
                SELECT ctid FROM events WHERE received_at < ?1 LIMIT ?2
            )"
        );

        // Collect distinct fingerprints from the rows about to be deleted.
        #[cfg(feature = "sqlite")]
        let fp_sql = "SELECT DISTINCT fingerprint FROM events \
             WHERE fingerprint IS NOT NULL AND rowid IN (\
                 SELECT rowid FROM events WHERE received_at < ?1 LIMIT ?2\
             )";
        #[cfg(not(feature = "sqlite"))]
        let fp_sql = "SELECT DISTINCT fingerprint FROM events \
             WHERE fingerprint IS NOT NULL AND ctid IN (\
                 SELECT ctid FROM events WHERE received_at < ?1 LIMIT ?2\
             )";

        let affected_fingerprints: Vec<String> = sqlx::query(fp_sql)
            .bind(cutoff)
            .bind(CHUNK_LIMIT)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|row| row.get::<String, _>(0))
            .collect();

        let deleted = sqlx::query(delete_sql)
            .bind(cutoff)
            .bind(CHUNK_LIMIT)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;

        if deleted == 0 {
            tx.rollback().await?;
            break;
        }

        if !affected_fingerprints.is_empty() {
            if let Err(e) = reconcile_affected_issues(&mut tx, &affected_fingerprints).await {
                tx.rollback().await?;
                return Err(e);
            }
        }

        tx.commit().await?;
        total_deleted += deleted;

        // If we got fewer than the limit we've caught up -- no need for another round.
        if deleted < CHUNK_LIMIT as usize {
            break;
        }

        // Brief pause between chunks to let the writer acquire the lock if it's waiting.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    total_deleted += delete_old_spans(pool, cutoff).await?;
    total_deleted += delete_old_metrics(pool, cutoff).await?;
    total_deleted += delete_old_logs(pool, cutoff).await?;

    // Vacuum freed pages outside any transaction so it doesn't hold the
    // write lock while reclaiming space.
    #[cfg(feature = "sqlite")]
    if total_deleted > 0 {
        if let Err(e) = sqlx::query("PRAGMA incremental_vacuum").execute(pool).await {
            tracing::warn!("retention: incremental_vacuum failed: {e}");
        }
    }

    Ok(total_deleted)
}

async fn delete_old_spans(pool: &DbPool, cutoff: i64) -> Result<usize> {
    let mut total = 0usize;
    loop {
        #[cfg(feature = "sqlite")]
        let delete_sql = sql!(
            "DELETE FROM spans WHERE rowid IN (
                SELECT rowid FROM spans WHERE received_at < ?1 LIMIT ?2
            )"
        );
        #[cfg(not(feature = "sqlite"))]
        let delete_sql = sql!(
            "DELETE FROM spans WHERE ctid IN (
                SELECT ctid FROM spans WHERE received_at < ?1 LIMIT ?2
            )"
        );

        let deleted = sqlx::query(delete_sql)
            .bind(cutoff)
            .bind(CHUNK_LIMIT)
            .execute(pool)
            .await?
            .rows_affected() as usize;

        total += deleted;

        if deleted < CHUNK_LIMIT as usize {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Ok(total)
}

async fn delete_old_metrics(pool: &DbPool, cutoff: i64) -> Result<usize> {
    let mut total = 0usize;
    loop {
        #[cfg(feature = "sqlite")]
        let delete_sql = sql!(
            "DELETE FROM metrics WHERE rowid IN (
                SELECT rowid FROM metrics WHERE received_at < ?1 LIMIT ?2
            )"
        );
        #[cfg(not(feature = "sqlite"))]
        let delete_sql = sql!(
            "DELETE FROM metrics WHERE ctid IN (
                SELECT ctid FROM metrics WHERE received_at < ?1 LIMIT ?2
            )"
        );

        let deleted = sqlx::query(delete_sql)
            .bind(cutoff)
            .bind(CHUNK_LIMIT)
            .execute(pool)
            .await?
            .rows_affected() as usize;

        total += deleted;

        if deleted < CHUNK_LIMIT as usize {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Ok(total)
}

async fn delete_old_logs(pool: &DbPool, cutoff: i64) -> Result<usize> {
    let mut total = 0usize;
    loop {
        #[cfg(feature = "sqlite")]
        let delete_sql = sql!(
            "DELETE FROM logs WHERE rowid IN (
                SELECT rowid FROM logs WHERE received_at < ?1 LIMIT ?2
            )"
        );
        #[cfg(not(feature = "sqlite"))]
        let delete_sql = sql!(
            "DELETE FROM logs WHERE ctid IN (
                SELECT ctid FROM logs WHERE received_at < ?1 LIMIT ?2
            )"
        );

        let deleted = sqlx::query(delete_sql)
            .bind(cutoff)
            .bind(CHUNK_LIMIT)
            .execute(pool)
            .await?
            .rows_affected() as usize;

        total += deleted;

        if deleted < CHUNK_LIMIT as usize {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Ok(total)
}

/// Reconcile issues touched by a retention delete -- remove orphans,
/// recount the rest, and vacuum the freed pages.
async fn reconcile_affected_issues(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    fingerprints: &[String],
) -> Result<()> {
    // Batch into chunks to stay within the DB's variable limit
    for chunk in fingerprints.chunks(500) {
        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
            "UPDATE issues SET event_count = (
                SELECT COUNT(*) FROM events WHERE events.fingerprint = issues.fingerprint
            ) WHERE fingerprint IN (",
        );
        let mut sep = qb.separated(", ");
        for fp in chunk {
            sep.push_bind(fp.as_str());
        }
        qb.push(")");
        qb.build().execute(&mut **tx).await?;

        // Delete tag values for ALL affected fingerprints — we can't recalculate
        // partial counts since tags aren't stored per-event in a queryable form.
        // The accumulator rebuilds from new incoming events.
        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
            "DELETE FROM issue_tag_values WHERE fingerprint IN (",
        );
        let mut sep = qb.separated(", ");
        for fp in chunk {
            sep.push_bind(fp.as_str());
        }
        qb.push(")");
        qb.build().execute(&mut **tx).await?;

        let mut qb =
            sqlx::QueryBuilder::<crate::db::Db>::new("DELETE FROM issues WHERE fingerprint IN (");
        let mut sep = qb.separated(", ");
        for fp in chunk {
            sep.push_bind(fp.as_str());
        }
        qb.push(") AND event_count = 0");
        qb.build().execute(&mut **tx).await?;
    }

    Ok(())
}

/// Targeted reconcile -- recount only the given fingerprints, remove orphans
/// among them, and vacuum. Opens its own transaction.
pub async fn reconcile_after_event_delete(pool: &DbPool, fingerprints: &[String]) -> Result<()> {
    if fingerprints.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    reconcile_affected_issues(&mut tx, fingerprints).await?;
    tx.commit().await?;
    Ok(())
}
