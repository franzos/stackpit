use anyhow::Result;
use sqlx::Row;

use crate::db::{sql, DbPool};

/// Per-chunk delete cap, keeps write-lock hold short for concurrent access.
const CHUNK_LIMIT: i64 = 5000;

pub async fn delete_old_events(pool: &DbPool, retention_days: u32) -> Result<usize> {
    let cutoff = chrono::Utc::now().timestamp() - (retention_days as i64 * 86400);
    let mut total_deleted: usize = 0;

    loop {
        let mut tx = pool.begin().await?;

        // Both subqueries share an identical WHERE + total ORDER BY (unique rowid/ctid) + LIMIT, so within one tx they select the exact same rows, keeping the reconciled fingerprints matched to the actually deleted rows.
        #[cfg(feature = "sqlite")]
        let delete_sql = sql!(
            "DELETE FROM events WHERE rowid IN (
                SELECT rowid FROM events WHERE received_at < ?1 ORDER BY received_at, rowid LIMIT ?2
            )"
        );
        #[cfg(not(feature = "sqlite"))]
        let delete_sql = sql!(
            "DELETE FROM events WHERE ctid IN (
                SELECT ctid FROM events WHERE received_at < ?1 ORDER BY received_at, ctid LIMIT ?2
            )"
        );

        // Fingerprints of the rows about to be deleted, for issue reconcile.
        #[cfg(feature = "sqlite")]
        let fp_sql = sql!(
            "SELECT DISTINCT fingerprint FROM events \
             WHERE fingerprint IS NOT NULL AND rowid IN (\
                 SELECT rowid FROM events WHERE received_at < ?1 ORDER BY received_at, rowid LIMIT ?2\
             )"
        );
        #[cfg(not(feature = "sqlite"))]
        let fp_sql = sql!(
            "SELECT DISTINCT fingerprint FROM events \
             WHERE fingerprint IS NOT NULL AND ctid IN (\
                 SELECT ctid FROM events WHERE received_at < ?1 ORDER BY received_at, ctid LIMIT ?2\
             )"
        );

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

        if deleted < CHUNK_LIMIT as usize {
            break;
        }

        // Pause between chunks to let a waiting writer acquire the lock.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    total_deleted += delete_old_spans(pool, cutoff).await?;
    total_deleted += delete_old_metrics(pool, cutoff).await?;
    total_deleted += delete_old_logs(pool, cutoff).await?;
    total_deleted += delete_old_transaction_metrics(pool, cutoff).await?;

    // Vacuum outside any transaction so it doesn't hold the write lock.
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

/// Drop transaction rollup rows whose most recent activity predates the cutoff.
/// Rolled-up rows have no `received_at`, so we use `last_seen` (event time).
async fn delete_old_transaction_metrics(pool: &DbPool, cutoff: i64) -> Result<usize> {
    let deleted = sqlx::query(sql!("DELETE FROM transaction_metrics WHERE last_seen < ?1"))
        .bind(cutoff)
        .execute(pool)
        .await?
        .rows_affected() as usize;
    Ok(deleted)
}

/// Reconcile issues touched by a retention delete: remove orphans and recount the rest.
async fn reconcile_affected_issues(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    fingerprints: &[String],
) -> Result<()> {
    // Chunk to stay within the DB's bind-variable limit.
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

        // Drop tag values for all affected fingerprints: counts can't be partially
        // recalculated (tags aren't per-event queryable); the accumulator rebuilds them.
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

/// Targeted reconcile of the given fingerprints in its own transaction.
pub async fn reconcile_after_event_delete(pool: &DbPool, fingerprints: &[String]) -> Result<()> {
    if fingerprints.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    reconcile_affected_issues(&mut tx, fingerprints).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sql;
    use crate::queries::test_helpers::*;

    async fn insert_issue(pool: &DbPool, fingerprint: &str, event_count: i64) {
        sqlx::query(sql!(
            "INSERT INTO issues (fingerprint, project_id, title, level, first_seen, last_seen, event_count)
             VALUES (?1, 1, 'test', 'error', 0, 0, ?2)"
        ))
        .bind(fingerprint)
        .bind(event_count)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn issue_count(pool: &DbPool, fingerprint: &str) -> Option<i64> {
        sqlx::query_scalar::<_, i64>(sql!(
            "SELECT event_count FROM issues WHERE fingerprint = ?1"
        ))
        .bind(fingerprint)
        .fetch_optional(pool)
        .await
        .unwrap()
    }

    async fn event_rows(pool: &DbPool) -> i64 {
        sqlx::query_scalar::<_, i64>(sql!("SELECT COUNT(*) FROM events"))
            .fetch_one(pool)
            .await
            .unwrap()
    }

    // Guards the reconcile-correctness contract: fingerprints reconciled after a delete must exactly cover the deleted rows; asserts the happy-path invariant, since the real divergence is a nondeterministic LIMIT-scan race.
    #[tokio::test]
    async fn delete_old_events_reconciles_affected_issues() {
        let pool = open_test_db().await;
        let old = 1_000; // received_at well before any sane cutoff
        let now = chrono::Utc::now().timestamp();

        // fp1: two old events (deleted), one recent event (survives) -> count 1.
        insert_test_event(&pool, "e1", 1, old, Some("fp1"), Some("error"), None).await;
        insert_test_event(&pool, "e2", 1, old, Some("fp1"), Some("error"), None).await;
        insert_test_event(&pool, "e3", 1, now, Some("fp1"), Some("error"), None).await;
        insert_issue(&pool, "fp1", 3).await;

        // fp2: two old events, all deleted -> issue drops.
        insert_test_event(&pool, "e4", 1, old, Some("fp2"), Some("error"), None).await;
        insert_test_event(&pool, "e5", 1, old, Some("fp2"), Some("error"), None).await;
        insert_issue(&pool, "fp2", 2).await;

        let deleted = delete_old_events(&pool, 1).await.unwrap();

        assert_eq!(deleted, 4);
        assert_eq!(event_rows(&pool).await, 1);
        assert_eq!(issue_count(&pool, "fp1").await, Some(1));
        assert_eq!(issue_count(&pool, "fp2").await, None); // reconciled to 0, dropped
    }
}
