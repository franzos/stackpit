use std::collections::HashMap;
use std::fmt::Write as _;

use anyhow::Result;
use sqlx::Row;

use crate::config::{Category, Resolved, MAX_OVERRIDE_DAYS};
use crate::db::{dyn_sql, sql, Db, DbPool};
use crate::ingest::models::ItemType;

/// Per-chunk delete cap, keeps write-lock hold short for concurrent access.
const CHUNK_LIMIT: i64 = 5000;

#[cfg(feature = "sqlite")]
const ROW_ID: &str = "rowid";
#[cfg(not(feature = "sqlite"))]
const ROW_ID: &str = "ctid";

/// Total order over `events` that keeps both planners on `idx_events_received`.
#[cfg(feature = "sqlite")]
const EVENTS_ORDER: &str = "received_at, rowid";
#[cfg(not(feature = "sqlite"))]
const EVENTS_ORDER: &str = "received_at, ctid";

/// Unary `+` keeps SQLite on `idx_events_received` instead of sorting via `idx_events_type_time`.
#[cfg(feature = "sqlite")]
const SERVER_ITEM_TYPE: &str = "+item_type";
#[cfg(not(feature = "sqlite"))]
const SERVER_ITEM_TYPE: &str = "item_type";

/// `events` item types owned by the errors category.
const ERROR_TYPES: [ItemType; 2] = [ItemType::Event, ItemType::UserReport];
/// `events` item types owned by the traces category.
const TRACE_TYPES: [ItemType; 3] = [
    ItemType::Transaction,
    ItemType::Profile,
    ItemType::ProfileChunk,
];

#[derive(Debug, Clone, Copy)]
enum Scope {
    Server,
    Project(i64),
}

/// Ingest-materialized releases with no surviving reference; releases with deploy metadata are kept.
const PRUNABLE_RELEASES: &str = "COALESCE(last_event, date_released, created_at) < ?1 \
     AND commit_sha IS NULL AND date_released IS NULL \
     AND NOT EXISTS (SELECT 1 FROM events e \
         WHERE e.project_id = releases.project_id AND e.release = releases.version) \
     AND NOT EXISTS (SELECT 1 FROM session_aggregates s \
         WHERE s.project_id = releases.project_id AND s.release = releases.version)";

/// Run the retention sweep under `policy`; returns the number of rows deleted.
pub async fn run_sweep(pool: &DbPool, policy: &Resolved) -> Result<usize> {
    let now = chrono::Utc::now().timestamp();
    let mut total = server_pass(pool, policy, now).await?;
    total += override_pass(pool, policy, now).await?;

    // Runs after every events and session_aggregates delete so references are final.
    if let Some(d) = policy.releases {
        total += prune_releases(pool, now - i64::from(d) * 86_400).await?;
    }

    #[cfg(feature = "sqlite")]
    if total > 0 {
        if let Err(e) = incremental_vacuum(pool).await {
            tracing::warn!("retention: incremental_vacuum failed: {e}");
        }
    }

    Ok(total)
}

/// Delete every row past its category's server cutoff, by `received_at`.
async fn server_pass(pool: &DbPool, policy: &Resolved, now: i64) -> Result<usize> {
    let cutoff = |days: u32| now - i64::from(days) * 86_400;
    let mut total = 0;

    if policy.events_uniform() {
        if let Some(d) = policy.base {
            total += delete_events_reconciled(
                pool,
                "received_at < ?1",
                EVENTS_ORDER,
                Scope::Server,
                cutoff(d),
            )
            .await?;
        }
    } else {
        let [errors, traces, rest] = item_type_predicates(SERVER_ITEM_TYPE);
        if let Some(d) = policy.errors {
            let pred = format!("received_at < ?1 AND {errors}");
            total += delete_events_reconciled(pool, &pred, EVENTS_ORDER, Scope::Server, cutoff(d))
                .await?;
        }
        if let Some(d) = policy.traces {
            let pred = format!("received_at < ?1 AND {traces}");
            total += chunked_delete(
                pool,
                "events",
                &pred,
                Some(EVENTS_ORDER),
                Scope::Server,
                cutoff(d),
            )
            .await?;
        }
        if let Some(d) = policy.base {
            let pred = format!("received_at < ?1 AND {rest}");
            total += chunked_delete(
                pool,
                "events",
                &pred,
                Some(EVENTS_ORDER),
                Scope::Server,
                cutoff(d),
            )
            .await?;
        }
    }

    if let Some(d) = policy.traces {
        total += chunked_delete(
            pool,
            "spans",
            "received_at < ?1",
            None,
            Scope::Server,
            cutoff(d),
        )
        .await?;
    }
    if let Some(d) = policy.logs {
        total += chunked_delete(
            pool,
            "logs",
            "received_at < ?1",
            None,
            Scope::Server,
            cutoff(d),
        )
        .await?;
    }
    if let Some(d) = policy.metrics {
        total += chunked_delete(
            pool,
            "metrics",
            "received_at < ?1",
            None,
            Scope::Server,
            cutoff(d),
        )
        .await?;
        // Rollup rows have no `received_at`; `last_seen` is event time.
        total += chunked_delete(
            pool,
            "transaction_metrics",
            "last_seen < ?1",
            None,
            Scope::Server,
            cutoff(d),
        )
        .await?;
    }
    if let Some(d) = policy.base {
        // `date` is a `%Y-%m-%d` string, not an epoch timestamp.
        if let Some(date) = chrono::DateTime::from_timestamp(cutoff(d), 0) {
            let date = date.format("%Y-%m-%d").to_string();
            total += chunked_delete(
                pool,
                "discard_stats",
                "date < ?1",
                None,
                Scope::Server,
                date.as_str(),
            )
            .await?;
        }
        // Whole days only: the bucket holding the cutoff still has rows inside the window.
        let day_cutoff = cutoff(d).div_euclid(86_400) * 86_400;
        total += chunked_delete(
            pool,
            "session_aggregates",
            "day_bucket < ?1",
            None,
            Scope::Server,
            day_cutoff,
        )
        .await?;
    }

    Ok(total)
}

/// `timestamp` bounds the index range; `received_at` spares freshly ingested backdated rows.
const OVERRIDE_PAST: &str = "timestamp < ?1 AND received_at < ?1";

/// Matches `idx_events_project_type_time`; ordering by `timestamp` alone sends both planners elsewhere.
const OVERRIDE_EVENTS_ORDER: &str = "item_type, timestamp DESC";

/// Delete each project's rows past its override where the override is shorter than the server value.
async fn override_pass(pool: &DbPool, policy: &Resolved, now: i64) -> Result<usize> {
    let [errors, traces, _] = item_type_predicates("item_type");
    let errors = format!("{OVERRIDE_PAST} AND {errors}");
    let traces = format!("{OVERRIDE_PAST} AND {traces}");
    let mut total = 0;

    for (project_id, cat, days) in list_overrides(pool).await? {
        if policy.days(cat).is_some_and(|server| server <= days) {
            continue;
        }
        let cutoff = now - i64::from(days) * 86_400;
        let scope = Scope::Project(project_id);
        let delete = |table, where_sql, order| {
            chunked_delete(pool, table, where_sql, Some(order), scope, cutoff)
        };
        match cat {
            Category::Errors => {
                total +=
                    delete_events_reconciled(pool, &errors, OVERRIDE_EVENTS_ORDER, scope, cutoff)
                        .await?;
            }
            Category::Traces => {
                total += delete("events", &traces, OVERRIDE_EVENTS_ORDER).await?;
                total += delete("spans", OVERRIDE_PAST, "timestamp").await?;
            }
            Category::Logs => total += delete("logs", OVERRIDE_PAST, "timestamp").await?,
            Category::Metrics => {
                total += delete("metrics", OVERRIDE_PAST, "timestamp").await?;
                // A bucket qualifies only once its whole hour is past the cutoff.
                total += chunked_delete(
                    pool,
                    "transaction_metrics",
                    "hour_bucket < ?1",
                    Some("hour_bucket"),
                    scope,
                    cutoff.div_euclid(3600) * 3600,
                )
                .await?;
            }
            Category::Releases => {}
        }
    }

    Ok(total)
}

async fn prune_releases(pool: &DbPool, cutoff: i64) -> Result<usize> {
    chunked_delete(
        pool,
        "releases",
        PRUNABLE_RELEASES,
        None,
        Scope::Server,
        cutoff,
    )
    .await
}

/// Errors, traces and remaining `events` predicates; inlined values are `ItemType` names, never input.
fn item_type_predicates(column: &str) -> [String; 3] {
    let list = |types: &[ItemType]| {
        let mut out = String::new();
        for (i, t) in types.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            let _ = write!(out, "'{}'", t.as_str());
        }
        out
    };
    let errors = list(&ERROR_TYPES);
    let traces = list(&TRACE_TYPES);
    [
        format!("{column} IN ({errors})"),
        format!("{column} IN ({traces})"),
        format!("{column} NOT IN ({errors}, {traces})"),
    ]
}

/// Binds `?1` cutoff, `?2` chunk limit, `?3` project id under `Scope::Project`.
fn chunk_select_sql(table: &str, where_sql: &str, order_sql: Option<&str>, scope: Scope) -> String {
    let mut sql = format!("SELECT {ROW_ID} FROM {table} WHERE ({where_sql})");
    if let Scope::Project(_) = scope {
        sql.push_str(" AND project_id = ?3");
    }
    if let Some(order) = order_sql {
        let _ = write!(sql, " ORDER BY {order}");
    }
    sql.push_str(" LIMIT ?2");
    sql
}

fn chunk_delete_sql(
    table: &str,
    where_sql: &str,
    order_sql: Option<&str>,
    scope: Scope,
    returning: Option<&str>,
) -> String {
    let inner = chunk_select_sql(table, where_sql, order_sql, scope);
    let mut sql = format!("DELETE FROM {table} WHERE {ROW_ID} IN ({inner})");
    if let Some(cols) = returning {
        let _ = write!(sql, " RETURNING {cols}");
    }
    sql
}

fn bind_chunk<'q, C>(
    sql: &str,
    scope: Scope,
    cutoff: C,
) -> sqlx::query::Query<'q, Db, <Db as sqlx::Database>::Arguments>
where
    C: for<'t> sqlx::Encode<'t, Db> + sqlx::Type<Db>,
{
    let query = sqlx::query(dyn_sql(sql)).bind(cutoff).bind(CHUNK_LIMIT);
    match scope {
        Scope::Server => query,
        Scope::Project(project_id) => query.bind(project_id),
    }
}

/// Pause between chunks to let a waiting writer acquire the lock.
async fn pause() {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

/// Delete matching rows of `table` in chunks of `CHUNK_LIMIT`, one statement each.
async fn chunked_delete<C>(
    pool: &DbPool,
    table: &str,
    where_sql: &str,
    order_sql: Option<&str>,
    scope: Scope,
    cutoff: C,
) -> Result<usize>
where
    C: Copy + Send + Sync + for<'t> sqlx::Encode<'t, Db> + sqlx::Type<Db>,
{
    let sql = chunk_delete_sql(table, where_sql, order_sql, scope, None);
    let mut total = 0usize;
    loop {
        let deleted = bind_chunk(&sql, scope, cutoff)
            .execute(pool)
            .await?
            .rows_affected() as usize;
        total += deleted;
        if deleted < CHUNK_LIMIT as usize {
            return Ok(total);
        }
        pause().await;
    }
}

/// `chunked_delete` on `events`, reconciling each chunk's issues in the same transaction.
async fn delete_events_reconciled(
    pool: &DbPool,
    where_sql: &str,
    order_sql: &str,
    scope: Scope,
    cutoff: i64,
) -> Result<usize> {
    let sql = chunk_delete_sql(
        "events",
        where_sql,
        Some(order_sql),
        scope,
        Some("project_id, fingerprint"),
    );
    let mut total = 0usize;
    loop {
        let mut tx = pool.begin().await?;
        let rows = bind_chunk(&sql, scope, cutoff).fetch_all(&mut *tx).await?;
        let deleted = rows.len();
        if deleted == 0 {
            tx.rollback().await?;
            return Ok(total);
        }

        let mut issue_keys: Vec<(i64, String)> = rows
            .iter()
            .filter_map(|row| {
                row.get::<Option<String>, _>(1)
                    .map(|fp| (row.get::<i64, _>(0), fp))
            })
            .collect();
        issue_keys.sort_unstable();
        issue_keys.dedup();
        if !issue_keys.is_empty() {
            reconcile_affected_issues(&mut tx, &issue_keys).await?;
        }

        tx.commit().await?;
        total += deleted;
        if deleted < CHUNK_LIMIT as usize {
            return Ok(total);
        }
        pause().await;
    }
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

/// Validate a stored override row; `None` (with a warning) for anything the
/// current code can't apply.
fn decode_override(project_id: i64, category: &str, days: i64) -> Option<(Category, u32)> {
    let Ok(cat) = category.parse::<Category>() else {
        tracing::warn!(
            "retention: project {project_id} has unknown override category `{category}`, skipped"
        );
        return None;
    };
    match u32::try_from(days) {
        Ok(d) if d > 0 && d <= MAX_OVERRIDE_DAYS => Some((cat, d)),
        _ => {
            tracing::warn!(
                "retention: project {project_id} has out-of-range {category} override {days}, skipped"
            );
            None
        }
    }
}

/// All valid override rows as `(project_id, category, days)`.
pub async fn list_overrides(pool: &DbPool) -> Result<Vec<(i64, Category, u32)>> {
    let rows = sqlx::query(sql!(
        "SELECT project_id, category, days FROM project_retention ORDER BY project_id, category"
    ))
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let project_id: i64 = row.get(0);
            let category: String = row.get(1);
            decode_override(project_id, &category, row.get(2))
                .map(|(cat, days)| (project_id, cat, days))
        })
        .collect())
}

pub async fn get_overrides(pool: &DbPool, project_id: i64) -> Result<HashMap<Category, u32>> {
    let rows = sqlx::query(sql!(
        "SELECT category, days FROM project_retention WHERE project_id = ?1"
    ))
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let category: String = row.get(0);
            decode_override(project_id, &category, row.get(1))
        })
        .collect())
}

/// Apply a set of override changes atomically: `Some(days)` sets, `None` clears.
pub async fn apply_overrides(
    pool: &DbPool,
    project_id: i64,
    changes: &[(Category, Option<u32>)],
) -> Result<()> {
    let mut tx = pool.begin().await?;
    for (cat, days) in changes {
        match days {
            Some(days) => {
                sqlx::query(sql!(
                    "INSERT INTO project_retention (project_id, category, days) VALUES (?1, ?2, ?3) \
                     ON CONFLICT(project_id, category) DO UPDATE SET days = excluded.days"
                ))
                .bind(project_id)
                .bind(cat.as_str())
                .bind(i64::from(*days))
                .execute(&mut *tx)
                .await?;
            }
            None => {
                sqlx::query(sql!(
                    "DELETE FROM project_retention WHERE project_id = ?1 AND category = ?2"
                ))
                .bind(project_id)
                .bind(cat.as_str())
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

pub async fn count_projects_with_overrides(pool: &DbPool) -> Result<i64> {
    let n: i64 = sqlx::query_scalar(sql!(
        "SELECT COUNT(DISTINCT project_id) FROM project_retention"
    ))
    .fetch_one(pool)
    .await?;
    Ok(n)
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

    // Reconciled issue keys must exactly cover the deleted rows.
    #[tokio::test]
    async fn run_sweep_reconciles_affected_issues() {
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

        let deleted = run_sweep(&pool, &Resolved::uniform(1)).await.unwrap();

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

        assert_eq!(run_sweep(&pool, &Resolved::uniform(1)).await.unwrap(), 2);
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

        assert_eq!(run_sweep(&pool, &Resolved::uniform(7)).await.unwrap(), 1);

        let remaining: Vec<String> =
            sqlx::query_scalar::<_, String>(sql!("SELECT date FROM discard_stats"))
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(remaining, vec![today]);
    }

    async fn insert_raw_override(pool: &DbPool, project_id: i64, category: &str, days: i64) {
        sqlx::query(sql!(
            "INSERT INTO project_retention (project_id, category, days) VALUES (?1, ?2, ?3)"
        ))
        .bind(project_id)
        .bind(category)
        .bind(days)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn apply_overrides_upserts_updates_and_clears() {
        let pool = open_test_db().await;

        apply_overrides(
            &pool,
            1,
            &[(Category::Logs, Some(7)), (Category::Traces, Some(14))],
        )
        .await
        .unwrap();
        let got = get_overrides(&pool, 1).await.unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[&Category::Logs], 7);
        assert_eq!(got[&Category::Traces], 14);

        apply_overrides(
            &pool,
            1,
            &[(Category::Logs, Some(3)), (Category::Traces, None)],
        )
        .await
        .unwrap();
        let got = get_overrides(&pool, 1).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[&Category::Logs], 3);

        // Clearing an absent override is a no-op.
        apply_overrides(&pool, 1, &[(Category::Metrics, None)])
            .await
            .unwrap();
        assert_eq!(get_overrides(&pool, 1).await.unwrap().len(), 1);
    }

    // A failing statement mid-batch must leave none of the batch applied.
    #[tokio::test]
    async fn apply_overrides_is_one_transaction() {
        let pool = open_test_db().await;
        apply_overrides(&pool, 1, &[(Category::Errors, Some(30))])
            .await
            .unwrap();

        let res = apply_overrides(
            &pool,
            1,
            &[
                (Category::Errors, None),
                (Category::Logs, Some(7)),
                (Category::Traces, Some(0)),
            ],
        )
        .await;
        assert!(res.is_err(), "days = 0 must violate the CHECK constraint");

        let got = get_overrides(&pool, 1).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[&Category::Errors], 30);
    }

    #[tokio::test]
    async fn list_and_count_scope_by_project() {
        let pool = open_test_db().await;
        apply_overrides(
            &pool,
            1,
            &[(Category::Logs, Some(7)), (Category::Metrics, Some(5))],
        )
        .await
        .unwrap();
        apply_overrides(&pool, 2, &[(Category::Errors, Some(10))])
            .await
            .unwrap();

        assert_eq!(
            list_overrides(&pool).await.unwrap(),
            vec![
                (1, Category::Logs, 7),
                (1, Category::Metrics, 5),
                (2, Category::Errors, 10),
            ]
        );
        assert_eq!(count_projects_with_overrides(&pool).await.unwrap(), 2);
        assert_eq!(get_overrides(&pool, 2).await.unwrap().len(), 1);
        assert!(get_overrides(&pool, 3).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_or_out_of_range_rows_are_skipped() {
        let pool = open_test_db().await;
        insert_raw_override(&pool, 1, "bogus", 7).await;
        insert_raw_override(&pool, 1, "logs", i64::from(MAX_OVERRIDE_DAYS) + 1).await;
        insert_raw_override(&pool, 1, "traces", 9).await;

        assert_eq!(
            list_overrides(&pool).await.unwrap(),
            vec![(1, Category::Traces, 9)]
        );
        let got = get_overrides(&pool, 1).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[&Category::Traces], 9);
    }

    const DAY: i64 = 86_400;

    const ALL_ITEM_TYPES: [ItemType; 17] = [
        ItemType::Event,
        ItemType::Transaction,
        ItemType::Session,
        ItemType::Sessions,
        ItemType::Attachment,
        ItemType::ClientReport,
        ItemType::CheckIn,
        ItemType::Profile,
        ItemType::ProfileChunk,
        ItemType::ReplayEvent,
        ItemType::ReplayRecording,
        ItemType::ReplayVideo,
        ItemType::UserReport,
        ItemType::Log,
        ItemType::Span,
        ItemType::Metric,
        ItemType::Unknown,
    ];

    fn forever() -> Resolved {
        Resolved::uniform(0)
    }

    fn day_string(ts: i64) -> String {
        chrono::DateTime::from_timestamp(ts, 0)
            .unwrap()
            .format("%Y-%m-%d")
            .to_string()
    }

    async fn event_ids(pool: &DbPool) -> Vec<String> {
        sqlx::query_scalar(sql!("SELECT event_id FROM events ORDER BY event_id"))
            .fetch_all(pool)
            .await
            .unwrap()
    }

    async fn insert_attachment(pool: &DbPool, event_id: &str) {
        sqlx::query(sql!(
            "INSERT INTO attachments (event_id, filename, data) VALUES (?1, 'a.txt', ?2)"
        ))
        .bind(event_id)
        .bind(&[0u8] as &[u8])
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_replay_metadata(pool: &DbPool, event_id: &str) {
        sqlx::query(sql!(
            "INSERT INTO replay_metadata (event_id, project_id) VALUES (?1, 1)"
        ))
        .bind(event_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn bulk_insert_error_events(pool: &DbPool, n: i64, received_at: i64, fingerprint: &str) {
        sqlx::query(sql!(
            "WITH RECURSIVE seq(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM seq WHERE x < ?1)
             INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, received_at, fingerprint)
             SELECT 'bulk-' || x, ?2, ?3, 1, 'testkey', ?4, ?4, ?5 FROM seq"
        ))
        .bind(n)
        .bind(ItemType::Event.as_str())
        .bind(&[0u8] as &[u8])
        .bind(received_at)
        .bind(fingerprint)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn bulk_insert_logs(pool: &DbPool, n: i64, received_at: i64) {
        sqlx::query(sql!(
            "WITH RECURSIVE seq(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM seq WHERE x < ?1)
             INSERT INTO logs (payload, project_id, public_key, timestamp, received_at)
             SELECT ?2, 1, 'testkey', ?3, ?3 FROM seq"
        ))
        .bind(n)
        .bind(&[0u8] as &[u8])
        .bind(received_at)
        .execute(pool)
        .await
        .unwrap();
    }

    // A uniform policy removes exactly the rows past the cutoff, in every table it prunes.
    #[tokio::test]
    async fn uniform_policy_prunes_exactly_the_legacy_rows() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 100 * DAY;

        for t in ALL_ITEM_TYPES {
            insert_typed_event(&pool, &format!("old-{}", t.as_str()), 1, t, old, old, None).await;
        }
        insert_typed_event(&pool, "new-event", 1, ItemType::Event, now, now, None).await;
        insert_typed_event(
            &pool,
            "new-replay",
            1,
            ItemType::ReplayEvent,
            now,
            now,
            None,
        )
        .await;
        insert_attachment(&pool, "old-event").await;
        insert_attachment(&pool, "new-event").await;
        insert_replay_metadata(&pool, "old-replay_event").await;
        insert_replay_metadata(&pool, "new-replay").await;

        insert_test_span(&pool, "s-old", 1, old, old).await;
        insert_test_span(&pool, "s-new", 1, now, now).await;
        insert_test_log(&pool, 1, old, old).await;
        insert_test_log(&pool, 1, now, now).await;
        insert_test_metric(&pool, 1, old, old).await;
        insert_test_metric(&pool, 1, now, now).await;
        insert_test_transaction_metric(&pool, 1, "/t", old - old % 3600, old).await;
        insert_test_transaction_metric(&pool, 1, "/t", now - now % 3600, now).await;
        insert_test_discard_stat(&pool, 1, &day_string(old)).await;
        insert_test_discard_stat(&pool, 1, &day_string(now)).await;

        let deleted = run_sweep(&pool, &Resolved::uniform(90)).await.unwrap();

        assert_eq!(deleted, ALL_ITEM_TYPES.len() + 5);
        assert_eq!(event_ids(&pool).await, vec!["new-event", "new-replay"]);
        for table in [
            "attachments",
            "replay_metadata",
            "spans",
            "logs",
            "metrics",
            "transaction_metrics",
            "discard_stats",
        ] {
            assert_eq!(count_rows(&pool, table).await, 1, "{table}");
        }
    }

    #[tokio::test]
    async fn errors_category_reconciles_and_spares_other_types() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 60 * DAY;

        insert_typed_event(&pool, "e1", 1, ItemType::Event, old, old, Some("fp1")).await;
        insert_typed_event(&pool, "e2", 1, ItemType::Event, old, old, Some("fp1")).await;
        insert_typed_event(&pool, "e3", 1, ItemType::Event, now, now, Some("fp1")).await;
        insert_issue(&pool, "fp1", 3).await;
        insert_typed_event(&pool, "e4", 1, ItemType::Event, old, old, Some("fp2")).await;
        insert_issue(&pool, "fp2", 1).await;
        insert_typed_event(&pool, "nofp", 1, ItemType::Event, old, old, None).await;
        insert_typed_event(&pool, "ur", 1, ItemType::UserReport, old, old, None).await;
        insert_typed_event(&pool, "tx", 1, ItemType::Transaction, old, old, None).await;
        insert_typed_event(&pool, "ses", 1, ItemType::Session, old, old, None).await;
        insert_attachment(&pool, "e4").await;
        insert_attachment(&pool, "e3").await;

        let policy = Resolved {
            errors: Some(30),
            ..forever()
        };
        assert_eq!(run_sweep(&pool, &policy).await.unwrap(), 5);

        assert_eq!(event_ids(&pool).await, vec!["e3", "ses", "tx"]);
        assert_eq!(issue_count(&pool, "fp1").await, Some(1));
        assert_eq!(issue_count(&pool, "fp2").await, None);
        assert_eq!(count_rows(&pool, "attachments").await, 1);
    }

    #[tokio::test]
    async fn traces_category_leaves_errors_alone() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 60 * DAY;

        insert_typed_event(&pool, "ev", 1, ItemType::Event, old, old, Some("fp")).await;
        insert_issue(&pool, "fp", 1).await;
        insert_typed_event(&pool, "tx-old", 1, ItemType::Transaction, old, old, None).await;
        insert_typed_event(&pool, "tx-new", 1, ItemType::Transaction, now, now, None).await;
        insert_typed_event(&pool, "prof", 1, ItemType::Profile, old, old, None).await;
        insert_typed_event(&pool, "chunk", 1, ItemType::ProfileChunk, old, old, None).await;
        insert_test_span(&pool, "s-old", 1, old, old).await;
        insert_test_span(&pool, "s-new", 1, now, now).await;
        insert_test_log(&pool, 1, old, old).await;

        let policy = Resolved {
            traces: Some(30),
            ..forever()
        };
        assert_eq!(run_sweep(&pool, &policy).await.unwrap(), 4);

        assert_eq!(event_ids(&pool).await, vec!["ev", "tx-new"]);
        assert_eq!(issue_count(&pool, "fp").await, Some(1));
        assert_eq!(count_rows(&pool, "spans").await, 1);
        assert_eq!(count_rows(&pool, "logs").await, 1);
    }

    #[tokio::test]
    async fn transaction_outlives_error_of_same_age_when_traces_longer() {
        let pool = open_test_db().await;
        let old = chrono::Utc::now().timestamp() - 100 * DAY;

        insert_typed_event(&pool, "ev", 1, ItemType::Event, old, old, None).await;
        insert_typed_event(&pool, "tx", 1, ItemType::Transaction, old, old, None).await;
        insert_typed_event(&pool, "ses", 1, ItemType::Session, old, old, None).await;
        insert_test_span(&pool, "s", 1, old, old).await;

        let policy = Resolved {
            base: Some(365),
            errors: Some(30),
            traces: Some(365),
            ..forever()
        };
        assert_eq!(run_sweep(&pool, &policy).await.unwrap(), 1);

        assert_eq!(event_ids(&pool).await, vec!["ses", "tx"]);
        assert_eq!(count_rows(&pool, "spans").await, 1);
    }

    #[tokio::test]
    async fn logs_category_prunes_only_logs() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 60 * DAY;

        insert_test_log(&pool, 1, old, old).await;
        insert_test_log(&pool, 1, now, now).await;
        insert_test_metric(&pool, 1, old, old).await;
        insert_typed_event(&pool, "ev", 1, ItemType::Event, old, old, None).await;

        let policy = Resolved {
            logs: Some(30),
            ..forever()
        };
        assert_eq!(run_sweep(&pool, &policy).await.unwrap(), 1);

        assert_eq!(count_rows(&pool, "logs").await, 1);
        assert_eq!(count_rows(&pool, "metrics").await, 1);
        assert_eq!(event_rows(&pool).await, 1);
    }

    #[tokio::test]
    async fn metrics_category_prunes_metrics_and_transaction_rollups() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 60 * DAY;

        insert_test_metric(&pool, 1, old, old).await;
        insert_test_metric(&pool, 1, now, now).await;
        insert_test_transaction_metric(&pool, 1, "/t", old - old % 3600, old).await;
        insert_test_transaction_metric(&pool, 1, "/t", now - now % 3600, now).await;
        insert_test_log(&pool, 1, old, old).await;

        let policy = Resolved {
            metrics: Some(30),
            ..forever()
        };
        assert_eq!(run_sweep(&pool, &policy).await.unwrap(), 2);

        assert_eq!(count_rows(&pool, "metrics").await, 1);
        assert_eq!(count_rows(&pool, "transaction_metrics").await, 1);
        assert_eq!(count_rows(&pool, "logs").await, 1);
    }

    // Remaining item types follow `base`; cascades follow the parent row's category.
    #[tokio::test]
    async fn remaining_event_types_follow_base_with_cascades() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 100 * DAY;

        let rest = [
            ItemType::Session,
            ItemType::Sessions,
            ItemType::Attachment,
            ItemType::ClientReport,
            ItemType::CheckIn,
            ItemType::ReplayEvent,
            ItemType::ReplayRecording,
            ItemType::ReplayVideo,
            ItemType::Unknown,
        ];
        for t in rest {
            insert_typed_event(&pool, t.as_str(), 1, t, old, old, None).await;
        }
        insert_typed_event(&pool, "ev", 1, ItemType::Event, old, old, None).await;
        insert_typed_event(&pool, "tx", 1, ItemType::Transaction, old, old, None).await;
        insert_typed_event(&pool, "ses-new", 1, ItemType::Session, now, now, None).await;
        insert_replay_metadata(&pool, "replay_event").await;
        insert_attachment(&pool, "unknown").await;
        insert_attachment(&pool, "ev").await;
        insert_test_discard_stat(&pool, 1, &day_string(old)).await;
        insert_test_discard_stat(&pool, 1, &day_string(now)).await;

        let policy = Resolved {
            base: Some(30),
            errors: Some(365),
            traces: Some(365),
            ..forever()
        };
        assert_eq!(run_sweep(&pool, &policy).await.unwrap(), rest.len() + 1);

        assert_eq!(event_ids(&pool).await, vec!["ev", "ses-new", "tx"]);
        assert_eq!(count_rows(&pool, "replay_metadata").await, 0);
        assert_eq!(count_rows(&pool, "attachments").await, 1);
        assert_eq!(count_rows(&pool, "discard_stats").await, 1);
    }

    #[tokio::test]
    async fn forever_policy_deletes_nothing() {
        let pool = open_test_db().await;
        let old = chrono::Utc::now().timestamp() - 1000 * DAY;

        for t in ALL_ITEM_TYPES {
            insert_typed_event(&pool, t.as_str(), 1, t, old, old, None).await;
        }
        insert_test_span(&pool, "s", 1, old, old).await;
        insert_test_log(&pool, 1, old, old).await;
        insert_test_metric(&pool, 1, old, old).await;
        insert_test_transaction_metric(&pool, 1, "/t", old - old % 3600, old).await;
        insert_test_discard_stat(&pool, 1, &day_string(old)).await;
        insert_test_session_aggregate(&pool, 1, "v1", old - old % DAY).await;
        insert_test_release(&pool, 1, "v1", None, None, Some(old), old).await;

        assert_eq!(run_sweep(&pool, &forever()).await.unwrap(), 0);

        assert_eq!(event_rows(&pool).await, ALL_ITEM_TYPES.len() as i64);
        for table in [
            "spans",
            "logs",
            "metrics",
            "transaction_metrics",
            "discard_stats",
            "session_aggregates",
            "releases",
        ] {
            assert_eq!(count_rows(&pool, table).await, 1, "{table}");
        }
    }

    #[tokio::test]
    async fn chunk_loops_continue_past_chunk_limit() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 60 * DAY;

        bulk_insert_error_events(&pool, CHUNK_LIMIT + 1, old, "fp-bulk").await;
        insert_typed_event(
            &pool,
            "fresh",
            1,
            ItemType::Event,
            now,
            now,
            Some("fp-bulk"),
        )
        .await;
        insert_issue(&pool, "fp-bulk", CHUNK_LIMIT + 2).await;
        bulk_insert_logs(&pool, CHUNK_LIMIT + 1, old).await;
        insert_test_log(&pool, 1, now, now).await;

        let policy = Resolved {
            errors: Some(30),
            logs: Some(30),
            ..forever()
        };
        let deleted = run_sweep(&pool, &policy).await.unwrap();

        assert_eq!(deleted, 2 * (CHUNK_LIMIT as usize + 1));
        assert_eq!(event_ids(&pool).await, vec!["fresh"]);
        assert_eq!(issue_count(&pool, "fp-bulk").await, Some(1));
        assert_eq!(count_rows(&pool, "logs").await, 1);
    }

    #[tokio::test]
    async fn project_scope_deletes_only_that_project() {
        let pool = open_test_db().await;
        let old = chrono::Utc::now().timestamp() - 60 * DAY;
        let cutoff = old + DAY;

        insert_typed_event(&pool, "a", 1, ItemType::Event, old, old, Some("fp")).await;
        insert_typed_event(&pool, "b", 2, ItemType::Event, old, old, Some("fp")).await;
        insert_issue_in(&pool, 1, "fp", 1).await;
        insert_issue_in(&pool, 2, "fp", 1).await;
        insert_test_log(&pool, 1, old, old).await;
        insert_test_log(&pool, 2, old, old).await;

        let deleted = delete_events_reconciled(
            &pool,
            "received_at < ?1",
            EVENTS_ORDER,
            Scope::Project(1),
            cutoff,
        )
        .await
        .unwrap();
        assert_eq!(deleted, 1);
        let deleted = chunked_delete(
            &pool,
            "logs",
            "received_at < ?1",
            None,
            Scope::Project(1),
            cutoff,
        )
        .await
        .unwrap();
        assert_eq!(deleted, 1);

        assert_eq!(event_ids(&pool).await, vec!["b"]);
        assert_eq!(issue_count_in(&pool, 1, "fp").await, None);
        assert_eq!(issue_count_in(&pool, 2, "fp").await, Some(1));
        let left: Vec<i64> = sqlx::query_scalar(sql!("SELECT project_id FROM logs"))
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(left, vec![2]);
    }

    fn typed_event_selects() -> Vec<String> {
        item_type_predicates(SERVER_ITEM_TYPE)
            .iter()
            .map(|pred| {
                chunk_select_sql(
                    "events",
                    &format!("received_at < ?1 AND {pred}"),
                    Some(EVENTS_ORDER),
                    Scope::Server,
                )
            })
            .collect()
    }

    /// `n` old rows, a tenth errors and most of the rest transactions.
    async fn seed_mixed_events(pool: &DbPool, n: i64, received_at: i64) {
        sqlx::query(sql!(
            "WITH RECURSIVE seq(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM seq WHERE x < ?1)
             INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, received_at)
             SELECT 'mix-' || x,
                    CASE x % 10 WHEN 0 THEN 'event' WHEN 1 THEN 'session' WHEN 2 THEN 'check_in' ELSE 'transaction' END,
                    ?2, 1, 'testkey', ?3 + x, ?3 + x
             FROM seq"
        ))
        .bind(n)
        .bind(&[0u8] as &[u8])
        .bind(received_at)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn typed_event_plans(pool: &DbPool, cutoff: i64) -> Vec<(String, Vec<String>)> {
        #[cfg(feature = "sqlite")]
        let (explain, detail_col) = ("EXPLAIN QUERY PLAN", 3);
        #[cfg(not(feature = "sqlite"))]
        let (explain, detail_col) = ("EXPLAIN", 0);

        let mut plans = Vec::new();
        for select in typed_event_selects() {
            let rows = sqlx::query(dyn_sql(&format!("{explain} {select}")))
                .bind(cutoff)
                .bind(CHUNK_LIMIT)
                .fetch_all(pool)
                .await
                .unwrap();
            let plan = rows
                .iter()
                .map(|r| r.get::<String, _>(detail_col))
                .collect();
            plans.push((select, plan));
        }
        plans
    }

    #[tokio::test]
    async fn typed_event_passes_use_received_index() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();

        let mut plans = typed_event_plans(&pool, 0).await;
        seed_mixed_events(&pool, 20 * CHUNK_LIMIT, now - 60 * DAY).await;
        #[cfg(feature = "sqlite")]
        crate::db::sqlite_analyze(&pool).await.unwrap();
        #[cfg(not(feature = "sqlite"))]
        sqlx::query("ANALYZE events").execute(&pool).await.unwrap();
        plans.extend(typed_event_plans(&pool, now).await);

        for (select, plan) in plans {
            #[cfg(feature = "sqlite")]
            {
                assert!(
                    plan.iter().any(|d| d.contains("idx_events_received")),
                    "{select}: {plan:?}"
                );
                assert!(
                    !plan
                        .iter()
                        .any(|d| d.contains("TEMP B-TREE") || d.contains("idx_events_type_time")),
                    "{select}: {plan:?}"
                );
            }
            // Postgres may pick idx_events_type_time for a rare item type, a valid plan.
            #[cfg(not(feature = "sqlite"))]
            assert!(
                !plan.iter().any(|d| d.contains("Seq Scan on events")),
                "{select}: {plan:?}"
            );
        }
    }

    /// Server keeps 90 days; rows at 60 days are past a 30-day override but not the server cutoff.
    fn server_90() -> Resolved {
        Resolved::uniform(90)
    }

    #[tokio::test]
    async fn errors_override_prunes_one_project_and_reconciles() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let mid = now - 60 * DAY;

        for p in [1, 2] {
            let id = |s: &str| format!("{s}-{p}");
            insert_typed_event(
                &pool,
                &id("ev-old"),
                p,
                ItemType::Event,
                mid,
                mid,
                Some("fp"),
            )
            .await;
            insert_typed_event(
                &pool,
                &id("ev-new"),
                p,
                ItemType::Event,
                now,
                now,
                Some("fp"),
            )
            .await;
            insert_typed_event(&pool, &id("ur"), p, ItemType::UserReport, mid, mid, None).await;
            insert_typed_event(&pool, &id("tx"), p, ItemType::Transaction, mid, mid, None).await;
            insert_typed_event(&pool, &id("ses"), p, ItemType::Session, mid, mid, None).await;
            insert_issue_in(&pool, p, "fp", 2).await;
            insert_typed_event(
                &pool,
                &id("gone"),
                p,
                ItemType::Event,
                mid,
                mid,
                Some("fp-gone"),
            )
            .await;
            insert_issue_in(&pool, p, "fp-gone", 1).await;
        }
        apply_overrides(&pool, 1, &[(Category::Errors, Some(30))])
            .await
            .unwrap();

        assert_eq!(run_sweep(&pool, &server_90()).await.unwrap(), 3);

        assert_eq!(
            event_ids(&pool).await,
            vec![
                "ev-new-1", "ev-new-2", "ev-old-2", "gone-2", "ses-1", "ses-2", "tx-1", "tx-2",
                "ur-2"
            ]
        );
        assert_eq!(issue_count_in(&pool, 1, "fp").await, Some(1));
        assert_eq!(issue_count_in(&pool, 1, "fp-gone").await, None);
        assert_eq!(issue_count_in(&pool, 2, "fp").await, Some(2));
        assert_eq!(issue_count_in(&pool, 2, "fp-gone").await, Some(1));
    }

    #[tokio::test]
    async fn traces_override_prunes_one_project() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let mid = now - 60 * DAY;

        for p in [1, 2] {
            let id = |s: &str| format!("{s}-{p}");
            insert_typed_event(&pool, &id("tx"), p, ItemType::Transaction, mid, mid, None).await;
            insert_typed_event(&pool, &id("prof"), p, ItemType::Profile, mid, mid, None).await;
            insert_typed_event(
                &pool,
                &id("chunk"),
                p,
                ItemType::ProfileChunk,
                mid,
                mid,
                None,
            )
            .await;
            insert_typed_event(
                &pool,
                &id("tx-new"),
                p,
                ItemType::Transaction,
                now,
                now,
                None,
            )
            .await;
            insert_typed_event(&pool, &id("ev"), p, ItemType::Event, mid, mid, None).await;
            insert_test_span(&pool, &id("s-old"), p, mid, mid).await;
            insert_test_span(&pool, &id("s-new"), p, now, now).await;
            insert_test_log(&pool, p, mid, mid).await;
        }
        apply_overrides(&pool, 1, &[(Category::Traces, Some(30))])
            .await
            .unwrap();

        assert_eq!(run_sweep(&pool, &server_90()).await.unwrap(), 4);

        assert_eq!(
            event_ids(&pool).await,
            vec!["chunk-2", "ev-1", "ev-2", "prof-2", "tx-2", "tx-new-1", "tx-new-2"]
        );
        let spans: Vec<String> =
            sqlx::query_scalar(sql!("SELECT span_id FROM spans ORDER BY span_id"))
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(spans, vec!["s-new-1", "s-new-2", "s-old-2"]);
        assert_eq!(count_rows(&pool, "logs").await, 2);
    }

    async fn project_ids(pool: &DbPool, table: &str) -> Vec<i64> {
        sqlx::query_scalar(dyn_sql(&format!(
            "SELECT project_id FROM {table} ORDER BY project_id"
        )))
        .fetch_all(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn logs_and_metrics_overrides_prune_one_project() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let mid = now - 60 * DAY;
        let cutoff_hour = (now - 30 * DAY).div_euclid(3600) * 3600;

        for p in [1, 2] {
            insert_test_log(&pool, p, mid, mid).await;
            insert_test_metric(&pool, p, mid, mid).await;
            // `last_seen` inside the server window keeps pass 1 off these rollups.
            insert_test_transaction_metric(&pool, p, "/old", cutoff_hour - 3600, now).await;
            insert_test_transaction_metric(&pool, p, "/edge", cutoff_hour, now).await;
        }
        insert_test_log(&pool, 1, now, now).await;
        insert_test_metric(&pool, 1, now, now).await;
        apply_overrides(
            &pool,
            1,
            &[(Category::Logs, Some(30)), (Category::Metrics, Some(30))],
        )
        .await
        .unwrap();

        assert_eq!(run_sweep(&pool, &server_90()).await.unwrap(), 3);

        assert_eq!(project_ids(&pool, "logs").await, vec![1, 2]);
        assert_eq!(project_ids(&pool, "metrics").await, vec![1, 2]);
        let rollups: Vec<(i64, String)> = sqlx::query_as(sql!(
            "SELECT project_id, transaction_name FROM transaction_metrics ORDER BY project_id, transaction_name"
        ))
        .fetch_all(&pool)
        .await
        .unwrap();
        let rollups: Vec<(i64, &str)> = rollups.iter().map(|(p, n)| (*p, n.as_str())).collect();
        assert_eq!(rollups, vec![(1, "/edge"), (2, "/edge"), (2, "/old")]);
    }

    // Pass 1 already deletes whatever such an override would, so the pass is run alone.
    #[tokio::test]
    async fn override_not_shorter_than_server_is_skipped() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 100 * DAY;

        insert_typed_event(&pool, "ev", 1, ItemType::Event, old, old, None).await;
        insert_typed_event(&pool, "tx", 1, ItemType::Transaction, old, old, None).await;
        insert_test_span(&pool, "s", 1, old, old).await;
        insert_test_log(&pool, 1, old, old).await;
        insert_test_metric(&pool, 1, old, old).await;
        insert_test_transaction_metric(&pool, 1, "/t", old - old % 3600, old).await;
        for (cat, days) in [
            (Category::Errors, 30),
            (Category::Traces, 60),
            (Category::Logs, 30),
            (Category::Metrics, 30),
        ] {
            insert_raw_override(&pool, 1, cat.as_str(), days).await;
        }

        // Errors lowered below the stored override, the rest equal or shorter.
        let policy = Resolved {
            errors: Some(10),
            ..Resolved::uniform(30)
        };
        assert_eq!(override_pass(&pool, &policy, now).await.unwrap(), 0);

        assert_eq!(event_rows(&pool).await, 2);
        for table in ["spans", "logs", "metrics", "transaction_metrics"] {
            assert_eq!(count_rows(&pool, table).await, 1, "{table}");
        }
    }

    #[tokio::test]
    async fn override_applies_under_forever_server() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let mid = now - 60 * DAY;

        insert_typed_event(&pool, "a", 1, ItemType::Event, mid, mid, None).await;
        insert_typed_event(&pool, "b", 2, ItemType::Event, mid, mid, None).await;
        insert_test_log(&pool, 1, mid, mid).await;
        insert_test_log(&pool, 2, mid, mid).await;
        apply_overrides(
            &pool,
            1,
            &[(Category::Errors, Some(30)), (Category::Logs, Some(30))],
        )
        .await
        .unwrap();

        assert_eq!(run_sweep(&pool, &forever()).await.unwrap(), 2);

        assert_eq!(event_ids(&pool).await, vec!["b"]);
        assert_eq!(project_ids(&pool, "logs").await, vec![2]);
    }

    // Backdated or offline-queued data is kept until `received_at` passes the cutoff too.
    #[tokio::test]
    async fn override_needs_both_clocks_past_the_cutoff() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let mid = now - 60 * DAY;

        insert_typed_event(&pool, "backdated", 1, ItemType::Event, mid, now, None).await;
        insert_typed_event(&pool, "future-skew", 1, ItemType::Event, now, mid, None).await;
        insert_test_span(&pool, "s", 1, mid, now).await;
        insert_test_log(&pool, 1, mid, now).await;
        insert_test_metric(&pool, 1, mid, now).await;
        for cat in [
            Category::Errors,
            Category::Traces,
            Category::Logs,
            Category::Metrics,
        ] {
            apply_overrides(&pool, 1, &[(cat, Some(30))]).await.unwrap();
        }

        assert_eq!(run_sweep(&pool, &server_90()).await.unwrap(), 0);

        assert_eq!(event_ids(&pool).await, vec!["backdated", "future-skew"]);
        for table in ["spans", "logs", "metrics"] {
            assert_eq!(count_rows(&pool, table).await, 1, "{table}");
        }
    }

    #[tokio::test]
    async fn override_for_missing_project_is_harmless() {
        let pool = open_test_db().await;
        let mid = chrono::Utc::now().timestamp() - 60 * DAY;

        insert_typed_event(&pool, "a", 1, ItemType::Event, mid, mid, None).await;
        insert_test_log(&pool, 1, mid, mid).await;
        apply_overrides(
            &pool,
            999,
            &[(Category::Errors, Some(30)), (Category::Logs, Some(30))],
        )
        .await
        .unwrap();

        assert_eq!(run_sweep(&pool, &server_90()).await.unwrap(), 0);
        assert_eq!(event_ids(&pool).await, vec!["a"]);
        assert_eq!(count_rows(&pool, "logs").await, 1);
    }

    fn override_selects() -> Vec<String> {
        let [errors, traces, _] = item_type_predicates("item_type");
        let scoped = |table, where_sql: &str, order| {
            chunk_select_sql(table, where_sql, Some(order), Scope::Project(1))
        };
        vec![
            scoped(
                "events",
                &format!("{OVERRIDE_PAST} AND {errors}"),
                OVERRIDE_EVENTS_ORDER,
            ),
            scoped(
                "events",
                &format!("{OVERRIDE_PAST} AND {traces}"),
                OVERRIDE_EVENTS_ORDER,
            ),
            scoped("spans", OVERRIDE_PAST, "timestamp"),
            scoped("logs", OVERRIDE_PAST, "timestamp"),
            scoped("metrics", OVERRIDE_PAST, "timestamp"),
            scoped("transaction_metrics", "hour_bucket < ?1", "hour_bucket"),
        ]
    }

    async fn override_plans(pool: &DbPool, cutoff: i64) -> Vec<(String, Vec<String>)> {
        #[cfg(feature = "sqlite")]
        let (explain, detail_col) = ("EXPLAIN QUERY PLAN", 3);
        #[cfg(not(feature = "sqlite"))]
        let (explain, detail_col) = ("EXPLAIN", 0);

        let mut tx = pool.begin().await.unwrap();
        // On near-empty tables Postgres picks a seq scan regardless of indexes; disabling it
        // makes the plan show whether a usable index exists at all.
        #[cfg(not(feature = "sqlite"))]
        for set in [
            "SET LOCAL enable_seqscan = off",
            "SET LOCAL enable_sort = off",
        ] {
            sqlx::query(set).execute(&mut *tx).await.unwrap();
        }

        let mut plans = Vec::new();
        for select in override_selects() {
            let rows = sqlx::query(dyn_sql(&format!("{explain} {select}")))
                .bind(cutoff)
                .bind(CHUNK_LIMIT)
                .bind(1i64)
                .fetch_all(&mut *tx)
                .await
                .unwrap();
            let plan = rows
                .iter()
                .map(|r| r.get::<String, _>(detail_col))
                .collect();
            plans.push((select, plan));
        }
        plans
    }

    #[tokio::test]
    async fn override_passes_use_project_indexes() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();

        let mut plans = override_plans(&pool, 0).await;
        seed_mixed_events(&pool, 20 * CHUNK_LIMIT, now - 60 * DAY).await;
        #[cfg(feature = "sqlite")]
        crate::db::sqlite_analyze(&pool).await.unwrap();
        #[cfg(not(feature = "sqlite"))]
        sqlx::query("ANALYZE events").execute(&pool).await.unwrap();
        plans.extend(override_plans(&pool, now - 30 * DAY).await);

        for (select, plan) in plans {
            let table = select.split_whitespace().nth(3).unwrap_or_default();
            let index = match table {
                "events" => "idx_events_project_type_time".to_owned(),
                "transaction_metrics" => "idx_transaction_metrics_project".to_owned(),
                _ => format!("idx_{table}_project_time"),
            };
            #[cfg(feature = "sqlite")]
            assert!(
                plan.iter().any(|d| d.contains(&index))
                    && !plan.iter().any(|d| d.contains("TEMP B-TREE")),
                "{select}: {plan:?}"
            );
            // Postgres may pick any project index; only rule out the full scan and the sort.
            #[cfg(not(feature = "sqlite"))]
            {
                let _ = index;
                assert!(
                    !plan
                        .iter()
                        .any(|d| d.contains("Seq Scan") || d.contains("Sort")),
                    "{select}: {plan:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn session_aggregates_prune_whole_days_only() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let cutoff_day = (now - 30 * DAY).div_euclid(DAY) * DAY;

        insert_test_session_aggregate(&pool, 1, "old", cutoff_day - DAY).await;
        insert_test_session_aggregate(&pool, 1, "partial", cutoff_day).await;
        insert_test_session_aggregate(&pool, 1, "new", now.div_euclid(DAY) * DAY).await;

        let policy = Resolved {
            base: Some(30),
            ..forever()
        };
        assert_eq!(server_pass(&pool, &policy, now).await.unwrap(), 1);

        let left: Vec<String> = sqlx::query_scalar(sql!(
            "SELECT release FROM session_aggregates ORDER BY release"
        ))
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(left, vec!["new", "partial"]);
    }

    #[tokio::test]
    async fn releases_prune_only_unreferenced_auto_materialized_rows() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 100 * DAY;

        // insert_test_event tags every event with release `v1.0`.
        insert_test_event(&pool, "ev", 1, old, None, None, None).await;
        insert_test_session_aggregate(&pool, 1, "by-session", old - old % DAY).await;

        insert_test_release(&pool, 1, "orphan", None, None, Some(old), old).await;
        insert_test_release(&pool, 1, "v1.0", None, None, Some(old), old).await;
        insert_test_release(&pool, 2, "v1.0", None, None, Some(old), old).await;
        insert_test_release(&pool, 1, "by-session", None, None, Some(old), old).await;
        insert_test_release(&pool, 1, "with-sha", Some("abc123"), None, Some(old), old).await;
        insert_test_release(&pool, 1, "released", None, Some(old), Some(old), old).await;
        insert_test_release(&pool, 1, "null-last-old", None, None, None, old).await;
        insert_test_release(&pool, 1, "null-last-new", None, None, None, now).await;
        insert_test_release(&pool, 1, "recent", None, None, Some(now), old).await;

        let policy = Resolved {
            releases: Some(30),
            ..forever()
        };
        assert_eq!(run_sweep(&pool, &policy).await.unwrap(), 3);

        let left: Vec<(i64, String)> = sqlx::query_as(sql!(
            "SELECT project_id, version FROM releases ORDER BY project_id, version"
        ))
        .fetch_all(&pool)
        .await
        .unwrap();
        let left: Vec<(i64, &str)> = left.iter().map(|(p, v)| (*p, v.as_str())).collect();
        assert_eq!(
            left,
            vec![
                (1, "by-session"),
                (1, "null-last-new"),
                (1, "recent"),
                (1, "released"),
                (1, "v1.0"),
                (1, "with-sha"),
            ]
        );
    }

    // A release whose events were swept in the same run goes in that run.
    #[tokio::test]
    async fn releases_prune_sees_events_deleted_earlier_in_the_sweep() {
        let pool = open_test_db().await;
        let old = chrono::Utc::now().timestamp() - 100 * DAY;

        insert_test_event(&pool, "ev", 1, old, None, None, None).await;
        insert_test_release(&pool, 1, "v1.0", None, None, Some(old), old).await;

        assert_eq!(run_sweep(&pool, &Resolved::uniform(30)).await.unwrap(), 2);
        assert_eq!(count_rows(&pool, "releases").await, 0);
    }

    async fn release_prune_plan(pool: &DbPool, cutoff: i64) -> Vec<String> {
        #[cfg(feature = "sqlite")]
        let (explain, detail_col) = ("EXPLAIN QUERY PLAN", 3);
        #[cfg(not(feature = "sqlite"))]
        let (explain, detail_col) = ("EXPLAIN", 0);

        let select = chunk_select_sql("releases", PRUNABLE_RELEASES, None, Scope::Server);
        sqlx::query(dyn_sql(&format!("{explain} {select}")))
            .bind(cutoff)
            .bind(CHUNK_LIMIT)
            .fetch_all(pool)
            .await
            .unwrap()
            .iter()
            .map(|r| r.get::<String, _>(detail_col))
            .collect()
    }

    #[tokio::test]
    async fn releases_prune_uses_project_release_index() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 60 * DAY;

        // Same item_type mix as the received-index test: leftover Postgres stats must not skew its plans.
        seed_mixed_events(&pool, 20 * CHUNK_LIMIT, old).await;
        sqlx::query(sql!(
            "UPDATE events SET release = 'r' || CAST(timestamp % 200 AS TEXT)"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(sql!(
            "WITH RECURSIVE seq(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM seq WHERE x < ?1)
             INSERT INTO session_aggregates (project_id, release, environment, day_bucket, sessions_total, first_seen, last_seen)
             SELECT 1, 's' || CAST(x % 200 AS TEXT), 'production', x, 1, 0, 0 FROM seq"
        ))
        .bind(20 * CHUNK_LIMIT)
        .execute(&pool)
        .await
        .unwrap();
        for i in 0..400 {
            insert_test_release(&pool, 1, &format!("r{i}"), None, None, Some(old), old).await;
        }
        #[cfg(feature = "sqlite")]
        crate::db::sqlite_analyze(&pool).await.unwrap();
        #[cfg(not(feature = "sqlite"))]
        sqlx::query("ANALYZE events, releases, session_aggregates")
            .execute(&pool)
            .await
            .unwrap();

        let plan = release_prune_plan(&pool, now).await;
        assert!(
            plan.iter()
                .any(|d| d.contains("idx_events_project_release")),
            "{plan:?}"
        );
        assert!(
            plan.iter()
                .filter(|d| d.contains("SCAN ") || d.contains("Seq Scan"))
                .all(|d| d.contains("releases")),
            "{plan:?}"
        );
    }
}
