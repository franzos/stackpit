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

        // Issue keys of the rows about to be deleted, for issue reconcile.
        #[cfg(feature = "sqlite")]
        let fp_sql = sql!(
            "SELECT DISTINCT project_id, fingerprint FROM events \
             WHERE fingerprint IS NOT NULL AND rowid IN (\
                 SELECT rowid FROM events WHERE received_at < ?1 ORDER BY received_at, rowid LIMIT ?2\
             )"
        );
        #[cfg(not(feature = "sqlite"))]
        let fp_sql = sql!(
            "SELECT DISTINCT project_id, fingerprint FROM events \
             WHERE fingerprint IS NOT NULL AND ctid IN (\
                 SELECT ctid FROM events WHERE received_at < ?1 ORDER BY received_at, ctid LIMIT ?2\
             )"
        );

        let affected_fingerprints: Vec<(i64, String)> = sqlx::query(fp_sql)
            .bind(cutoff)
            .bind(CHUNK_LIMIT)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|row| (row.get::<i64, _>(0), row.get::<String, _>(1)))
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
    total_deleted += delete_old_discard_stats(pool, cutoff).await?;

    #[cfg(feature = "sqlite")]
    if total_deleted > 0 {
        if let Err(e) = incremental_vacuum(pool).await {
            tracing::warn!("retention: incremental_vacuum failed: {e}");
        }
    }

    Ok(total_deleted)
}

/// Pages reclaimed per vacuum batch; keeps each write-lock hold short.
#[cfg(feature = "sqlite")]
const VACUUM_BATCH_PAGES: i64 = 1000;

/// Reclaim freelist pages in bounded batches with pauses in between: an
/// unbounded `incremental_vacuum` holds the write lock for the whole run,
/// long enough to starve the ingest writer past its retry ceiling.
#[cfg(feature = "sqlite")]
pub(crate) async fn incremental_vacuum(pool: &DbPool) -> Result<()> {
    loop {
        let before: i64 = sqlx::query_scalar("PRAGMA freelist_count")
            .fetch_one(pool)
            .await?;
        if before == 0 {
            return Ok(());
        }
        sqlx::query(crate::db::dyn_sql(&format!(
            "PRAGMA incremental_vacuum({VACUUM_BATCH_PAGES})"
        )))
        .execute(pool)
        .await?;
        let after: i64 = sqlx::query_scalar("PRAGMA freelist_count")
            .fetch_one(pool)
            .await?;
        // No progress means the db can't shrink further (e.g. not auto_vacuum=incremental).
        if after == 0 || after >= before {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
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
    let mut total = 0usize;
    loop {
        #[cfg(feature = "sqlite")]
        let delete_sql = sql!(
            "DELETE FROM transaction_metrics WHERE rowid IN (
                SELECT rowid FROM transaction_metrics WHERE last_seen < ?1 LIMIT ?2
            )"
        );
        #[cfg(not(feature = "sqlite"))]
        let delete_sql = sql!(
            "DELETE FROM transaction_metrics WHERE ctid IN (
                SELECT ctid FROM transaction_metrics WHERE last_seen < ?1 LIMIT ?2
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

/// Drop discard counters older than the cutoff. `date` is a `%Y-%m-%d` string,
/// not an epoch timestamp like the sibling tables.
async fn delete_old_discard_stats(pool: &DbPool, cutoff: i64) -> Result<usize> {
    let Some(cutoff_date) = chrono::DateTime::from_timestamp(cutoff, 0) else {
        return Ok(0);
    };
    let cutoff_date = cutoff_date.format("%Y-%m-%d").to_string();

    let mut total = 0usize;
    loop {
        let deleted = sqlx::query(sql!(
            "DELETE FROM discard_stats WHERE id IN (
                SELECT id FROM discard_stats WHERE date < ?1 LIMIT ?2
            )"
        ))
        .bind(&cutoff_date)
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

/// Push `(project_id, fingerprint) IN ((?, ?), ...)` for `pairs`. Two binds per
/// pair, so callers chunk accordingly.
pub(crate) fn push_pair_in_list(
    qb: &mut sqlx::QueryBuilder<crate::db::Db>,
    pairs: &[(i64, String)],
) {
    qb.push("(project_id, fingerprint) IN (");
    let mut sep = qb.separated(", ");
    for (project_id, fingerprint) in pairs {
        sep.push("(")
            .push_bind_unseparated(*project_id)
            .push_unseparated(", ")
            .push_bind_unseparated(fingerprint.as_str())
            .push_unseparated(")");
    }
    qb.push(")");
}

/// Reconcile issues touched by a retention delete: remove orphans and recount the rest.
async fn reconcile_affected_issues(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    issue_keys: &[(i64, String)],
) -> Result<()> {
    // Chunk to stay within the DB's bind-variable limit.
    for chunk in issue_keys.chunks(500) {
        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
            "UPDATE issues SET event_count = (
                SELECT COUNT(*) FROM events
                WHERE events.project_id = issues.project_id AND events.fingerprint = issues.fingerprint
            ) WHERE ",
        );
        push_pair_in_list(&mut qb, chunk);
        qb.build().execute(&mut **tx).await?;

        // Drop tag values for all affected issues: counts can't be partially
        // recalculated (tags aren't per-event queryable); the accumulator rebuilds them.
        let mut qb =
            sqlx::QueryBuilder::<crate::db::Db>::new("DELETE FROM issue_tag_values WHERE ");
        push_pair_in_list(&mut qb, chunk);
        qb.build().execute(&mut **tx).await?;

        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new("DELETE FROM issues WHERE ");
        push_pair_in_list(&mut qb, chunk);
        qb.push(" AND event_count = 0");
        qb.build().execute(&mut **tx).await?;
    }

    Ok(())
}

/// Targeted reconcile of the given `(project_id, fingerprint)` keys in its own transaction.
pub async fn reconcile_after_event_delete(
    pool: &DbPool,
    fingerprints: &[(i64, String)],
) -> Result<()> {
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
        insert_issue_in(pool, 1, fingerprint, event_count).await;
    }

    async fn insert_issue_in(pool: &DbPool, project_id: i64, fingerprint: &str, event_count: i64) {
        sqlx::query(sql!(
            "INSERT INTO issues (fingerprint, project_id, title, level, first_seen, last_seen, event_count)
             VALUES (?1, ?2, 'test', 'error', 0, 0, ?3)"
        ))
        .bind(fingerprint)
        .bind(project_id)
        .bind(event_count)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn issue_count(pool: &DbPool, fingerprint: &str) -> Option<i64> {
        issue_count_in(pool, 1, fingerprint).await
    }

    async fn issue_count_in(pool: &DbPool, project_id: i64, fingerprint: &str) -> Option<i64> {
        sqlx::query_scalar::<_, i64>(sql!(
            "SELECT event_count FROM issues WHERE project_id = ?1 AND fingerprint = ?2"
        ))
        .bind(project_id)
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

    // The batched vacuum must terminate even when the db can't shrink
    // (freelist not draining, e.g. auto_vacuum off).
    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn incremental_vacuum_terminates() {
        let pool = open_test_db().await;
        insert_test_event(&pool, "e1", 1, 0, None, None, None).await;
        sqlx::query("DELETE FROM events")
            .execute(&pool)
            .await
            .unwrap();
        incremental_vacuum(&pool).await.unwrap();
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

    // A fingerprint shared across projects: sweeping project 1's old events must
    // recount and drop only project 1's issue, never touch project 2's.
    #[tokio::test]
    async fn reconcile_keys_issues_by_project_and_fingerprint() {
        let pool = open_test_db().await;
        let old = 1_000;
        let now = chrono::Utc::now().timestamp();

        insert_test_event(&pool, "a1", 1, old, Some("fp-shared"), Some("error"), None).await;
        insert_test_event(&pool, "a2", 1, old, Some("fp-shared"), Some("error"), None).await;
        insert_issue_in(&pool, 1, "fp-shared", 2).await;

        insert_test_event(&pool, "b1", 2, now, Some("fp-shared"), Some("error"), None).await;
        insert_test_event(&pool, "b2", 2, now, Some("fp-shared"), Some("error"), None).await;
        insert_issue_in(&pool, 2, "fp-shared", 2).await;
        sqlx::query(sql!(
            "INSERT INTO issue_tag_values (project_id, fingerprint, tag_key, tag_value, count)
             VALUES (1, 'fp-shared', 'k', 'v', 2), (2, 'fp-shared', 'k', 'v', 2)"
        ))
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(delete_old_events(&pool, 1).await.unwrap(), 2);
        assert_eq!(issue_count_in(&pool, 1, "fp-shared").await, None);
        assert_eq!(issue_count_in(&pool, 2, "fp-shared").await, Some(2));

        let tag_rows: Vec<i64> = sqlx::query_scalar(sql!(
            "SELECT project_id FROM issue_tag_values WHERE fingerprint = 'fp-shared'"
        ))
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(tag_rows, vec![2]);
    }

    #[tokio::test]
    async fn prunes_discard_stats_past_the_cutoff() {
        let pool = open_test_db().await;
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let old = (chrono::Utc::now() - chrono::Duration::days(30))
            .format("%Y-%m-%d")
            .to_string();

        for date in [&today, &old] {
            crate::queries::filters::upsert_discard_stats(&pool, 1, "rate_limit", None, date, 4)
                .await
                .unwrap();
        }

        let cutoff = chrono::Utc::now().timestamp() - 7 * 86400;
        assert_eq!(delete_old_discard_stats(&pool, cutoff).await.unwrap(), 1);

        let remaining: Vec<String> =
            sqlx::query_scalar::<_, String>(sql!("SELECT date FROM discard_stats"))
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(remaining, vec![today]);
    }
}
