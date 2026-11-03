pub mod alerts;
pub mod api_keys;
pub mod backfill;
pub mod bulk;
pub mod client_reports;
pub mod event_supplements;
pub mod event_sync;
pub mod event_writes;
pub mod events;
pub mod filters;
pub mod integration_exclusions;
pub mod integrations;
pub mod issue_links;
pub mod issues;
pub mod logs;
pub mod metrics;
pub mod monitors;
pub mod notify_queue;
pub mod orgs;
pub mod period;
pub mod profiles;
pub mod projects;
pub mod releases;
pub mod replays;
pub mod retention;
pub mod spans;
pub mod traces;
pub mod transactions;
pub mod types;
pub mod users;

pub use types::*;

// Public functions here (e.g. `issues::update_issue_status`) take `IssueStatus`
// by value, so the type needs a reachable public path.
pub use crate::domain::IssueStatus;

/// Wrap a search term as a `%term%` LIKE pattern, escaping the LIKE
/// metacharacters with `\`. Callers must keep the matching `ESCAPE '\\'` clause.
pub(crate) fn like_contains(needle: &str) -> String {
    let escaped = needle
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// Wrap a search term as a `term%` LIKE pattern, escaping the LIKE
/// metacharacters with `\`. Callers must keep the matching `ESCAPE '\\'` clause.
pub(crate) fn like_prefix(needle: &str) -> String {
    let escaped = needle
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("{escaped}%")
}

/// Length of a W3C/Sentry trace id in hex characters.
pub const TRACE_ID_LEN: usize = 32;

/// Shortest prefix accepted as a trace-id lookup. Below this a hex string is
/// far likelier to be a word someone meant to search for.
const TRACE_ID_MIN_PREFIX: usize = 8;

/// Read a string as a full or partial trace id, normalised for comparison.
/// `None` when it cannot be one, which is what keeps an ordinary search term
/// out of the trace path.
pub fn trace_id_candidate(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if !(TRACE_ID_MIN_PREFIX..=TRACE_ID_LEN).contains(&trimmed.len())
        || !trimmed.chars().all(|c| c.is_ascii_hexdigit())
    {
        return None;
    }
    // Stored ids are lowercase hex, and both `=` and PostgreSQL's `LIKE` are
    // case-sensitive.
    Some(trimmed.to_ascii_lowercase())
}

/// Push a trace-id match on `column`. A full id compares with `=` so it rides
/// the trace index; a shorter one falls back to a prefix `LIKE`, which does not.
/// `column` is `&'static str` so no caller can route input into the SQL text.
pub(crate) fn push_trace_id_predicate(
    qb: &mut sqlx::QueryBuilder<crate::db::Db>,
    column: &'static str,
    trace_id: &str,
) {
    qb.push(column);
    if trace_id.len() >= TRACE_ID_LEN {
        qb.push(" = ");
        qb.push_bind(trace_id.to_string());
    } else {
        qb.push(" LIKE ");
        qb.push_bind(like_prefix(trace_id));
        qb.push(" ESCAPE '\\'");
    }
}

/// Canonical form of an org-id list: sorted and deduped, so two callers with the
/// same entitlements produce the same key and the same SQL.
pub(crate) fn canonical_org_ids(mut ids: Vec<i64>) -> Vec<i64> {
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Push `<column> IN (SELECT project_id FROM projects WHERE org_id IN (...))`,
/// binding every id. The caller supplies its own `WHERE`/`AND`, and must have
/// short-circuited an empty list: `IN ()` is invalid SQL on both backends.
/// `project_column` is `&'static str` so no caller can route input into the SQL text.
pub(crate) fn push_org_scope_predicate(
    qb: &mut sqlx::QueryBuilder<crate::db::Db>,
    project_column: &'static str,
    org_ids: &[i64],
) {
    debug_assert!(!org_ids.is_empty(), "an empty org list must short-circuit");
    qb.push(project_column);
    qb.push(" IN (SELECT project_id FROM projects WHERE org_id IN (");
    let mut sep = qb.separated(", ");
    for id in org_ids {
        sep.push_bind(*id);
    }
    qb.push("))");
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use crate::db::{self, sql, DbPool};

    /// Spins up a throwaway test DB with the full schema applied.
    pub async fn open_test_db() -> DbPool {
        db::open_test_pool().await
    }

    /// Inserts a test event with a zstd-compressed payload.
    pub async fn insert_test_event(
        pool: &DbPool,
        event_id: &str,
        project_id: i64,
        timestamp: i64,
        fingerprint: Option<&str>,
        level: Option<&str>,
        title: Option<&str>,
    ) {
        let payload_json = serde_json::json!({
            "event_id": event_id,
            "message": title.unwrap_or("test event"),
        });
        let payload_bytes = serde_json::to_vec(&payload_json).unwrap();
        let compressed = zstd::encode_all(payload_bytes.as_slice(), 3).unwrap();

        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, project_id, public_key, timestamp, level, title, platform, release, environment, server_name, transaction_name, sdk_name, sdk_version, received_at, fingerprint)
             VALUES (?1, 'event', ?2, 'testkey', ?3, ?4, ?5, 'rust', 'v1.0', 'production', 'server1', '/api/test', 'sentry.rust', '0.1.0', ?3, ?6)",
        ))
        .bind(event_id)
        .bind(project_id)
        .bind(timestamp)
        .bind(level)
        .bind(title)
        .bind(fingerprint)
        .execute(pool)
        .await
        .unwrap();
        insert_test_payload(pool, event_id, &compressed).await;
    }

    /// Gives every event without an `event_payloads` row a one-byte payload.
    pub async fn insert_missing_test_payloads(pool: &DbPool) {
        sqlx::query(sql!(
            "INSERT INTO event_payloads (event_id, payload)
             SELECT event_id, ?1 FROM events
             WHERE event_id NOT IN (SELECT event_id FROM event_payloads)"
        ))
        .bind(&[0u8] as &[u8])
        .execute(pool)
        .await
        .unwrap();
    }

    /// Inserts the `event_payloads` row for an existing event.
    pub async fn insert_test_payload(pool: &DbPool, event_id: &str, payload: &[u8]) {
        sqlx::query(sql!(
            "INSERT INTO event_payloads (event_id, payload) VALUES (?1, ?2)"
        ))
        .bind(event_id)
        .bind(payload)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Inserts a test issue row.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_test_issue(
        pool: &DbPool,
        fingerprint: &str,
        project_id: i64,
        title: Option<&str>,
        level: Option<&str>,
        first_seen: i64,
        last_seen: i64,
        event_count: i64,
        status: &str,
    ) {
        sqlx::query(sql!(
            "INSERT INTO issues (fingerprint, project_id, title, level, first_seen, last_seen, event_count, status, item_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'event')",
        ))
        .bind(fingerprint)
        .bind(project_id)
        .bind(title)
        .bind(level)
        .bind(first_seen)
        .bind(last_seen)
        .bind(event_count)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Inserts an `events` row of `item_type` with independent clocks.
    pub async fn insert_typed_event(
        pool: &DbPool,
        event_id: &str,
        project_id: i64,
        item_type: crate::ingest::models::ItemType,
        timestamp: i64,
        received_at: i64,
        fingerprint: Option<&str>,
    ) {
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, project_id, public_key, timestamp, received_at, fingerprint)
             VALUES (?1, ?2, ?3, 'testkey', ?4, ?5, ?6)",
        ))
        .bind(event_id)
        .bind(item_type.as_str())
        .bind(project_id)
        .bind(timestamp)
        .bind(received_at)
        .bind(fingerprint)
        .execute(pool)
        .await
        .unwrap();
        insert_test_payload(pool, event_id, &[0u8]).await;
    }

    pub async fn insert_test_span(
        pool: &DbPool,
        span_id: &str,
        project_id: i64,
        timestamp: i64,
        received_at: i64,
    ) {
        sqlx::query(sql!(
            "INSERT INTO spans (span_id, project_id, public_key, timestamp, received_at)
             VALUES (?1, ?2, 'testkey', ?3, ?4)",
        ))
        .bind(span_id)
        .bind(project_id)
        .bind(timestamp)
        .bind(received_at)
        .execute(pool)
        .await
        .unwrap();
    }

    pub async fn insert_test_log(pool: &DbPool, project_id: i64, timestamp: i64, received_at: i64) {
        sqlx::query(sql!(
            "INSERT INTO logs (payload, project_id, public_key, timestamp, received_at)
             VALUES (?1, ?2, 'testkey', ?3, ?4)",
        ))
        .bind(&[0u8] as &[u8])
        .bind(project_id)
        .bind(timestamp)
        .bind(received_at)
        .execute(pool)
        .await
        .unwrap();
    }

    pub async fn insert_test_metric(
        pool: &DbPool,
        project_id: i64,
        timestamp: i64,
        received_at: i64,
    ) {
        sqlx::query(sql!(
            "INSERT INTO metrics (project_id, timestamp, received_at, mri, metric_type)
             VALUES (?1, ?2, ?3, 'c:test@none', 'c')",
        ))
        .bind(project_id)
        .bind(timestamp)
        .bind(received_at)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Inserts an hourly rollup row; `first_seen` equals `last_seen`.
    pub async fn insert_test_transaction_metric(
        pool: &DbPool,
        project_id: i64,
        transaction_name: &str,
        hour_bucket: i64,
        last_seen: i64,
    ) {
        sqlx::query(sql!(
            "INSERT INTO transaction_metrics (project_id, transaction_name, hour_bucket, count, first_seen, last_seen)
             VALUES (?1, ?2, ?3, 1, ?4, ?4)",
        ))
        .bind(project_id)
        .bind(transaction_name)
        .bind(hour_bucket)
        .bind(last_seen)
        .execute(pool)
        .await
        .unwrap();
    }

    pub async fn insert_test_session_aggregate(
        pool: &DbPool,
        project_id: i64,
        release: &str,
        day_bucket: i64,
    ) {
        sqlx::query(sql!(
            "INSERT INTO session_aggregates (project_id, release, environment, day_bucket, sessions_total, first_seen, last_seen)
             VALUES (?1, ?2, 'production', ?3, 1, ?3, ?3)",
        ))
        .bind(project_id)
        .bind(release)
        .bind(day_bucket)
        .execute(pool)
        .await
        .unwrap();
    }

    pub async fn insert_test_release(
        pool: &DbPool,
        project_id: i64,
        version: &str,
        commit_sha: Option<&str>,
        date_released: Option<i64>,
        last_event: Option<i64>,
        created_at: i64,
    ) {
        sqlx::query(sql!(
            "INSERT INTO releases (project_id, version, commit_sha, date_released, last_event, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        ))
        .bind(project_id)
        .bind(version)
        .bind(commit_sha)
        .bind(date_released)
        .bind(last_event)
        .bind(created_at)
        .execute(pool)
        .await
        .unwrap();
    }

    /// `date` is `%Y-%m-%d`.
    pub async fn insert_test_discard_stat(pool: &DbPool, project_id: u64, date: &str) {
        crate::queries::filters::upsert_discard_stats(
            pool,
            project_id,
            "rate_limit",
            None,
            date,
            1,
        )
        .await
        .unwrap();
    }

    /// Writes the `environment` / `release` tag rows the writer would have
    /// produced for the events already in the table.
    pub async fn derive_env_release_tags(pool: &DbPool) {
        for column in ["environment", "release"] {
            sqlx::query(db::dyn_sql(&format!(
                "INSERT INTO issue_tag_values (project_id, fingerprint, tag_key, tag_value, count)
                 SELECT project_id, fingerprint, '{column}', {column}, COUNT(*) FROM events
                 WHERE fingerprint IS NOT NULL AND {column} IS NOT NULL AND {column} <> ''
                 GROUP BY project_id, fingerprint, {column}
                 ON CONFLICT (project_id, fingerprint, tag_key, tag_value) DO UPDATE SET count = excluded.count"
            )))
            .execute(pool)
            .await
            .unwrap();
        }
    }

    pub async fn count_rows(pool: &DbPool, table: &'static str) -> i64 {
        sqlx::query_scalar(db::dyn_sql(&format!("SELECT COUNT(*) FROM {table}")))
            .fetch_one(pool)
            .await
            .unwrap()
    }

    pub type PlanQuery = sqlx::query::Query<'static, db::Db, <db::Db as sqlx::Database>::Arguments>;

    /// Plan detail lines of `select`: `EXPLAIN QUERY PLAN` on SQLite, `EXPLAIN` on Postgres.
    pub async fn query_plan<'c, E>(
        executor: E,
        select: &str,
        bind: impl FnOnce(PlanQuery) -> PlanQuery,
    ) -> Vec<String>
    where
        E: sqlx::Executor<'c, Database = db::Db>,
    {
        bind(sqlx::query(db::dyn_sql(&format!("{EXPLAIN} {select}"))))
            .fetch_all(executor)
            .await
            .unwrap()
            .iter()
            .map(plan_detail)
            .collect()
    }

    #[cfg(feature = "sqlite")]
    const EXPLAIN: &str = "EXPLAIN QUERY PLAN";
    #[cfg(not(feature = "sqlite"))]
    const EXPLAIN: &str = "EXPLAIN";

    fn plan_detail(row: &<db::Db as sqlx::Database>::Row) -> String {
        use sqlx::Row;
        #[cfg(feature = "sqlite")]
        return row.get(3);
        #[cfg(not(feature = "sqlite"))]
        return row.get(0);
    }

    /// Plan detail lines of the query `push` appends to the backend's `EXPLAIN` prefix.
    pub async fn builder_plan<'c, E>(
        executor: E,
        push: impl FnOnce(&mut sqlx::QueryBuilder<db::Db>),
    ) -> Vec<String>
    where
        E: sqlx::Executor<'c, Database = db::Db>,
    {
        let mut qb = sqlx::QueryBuilder::<db::Db>::new(format!("{EXPLAIN} "));
        push(&mut qb);
        qb.build()
            .fetch_all(executor)
            .await
            .unwrap()
            .iter()
            .map(plan_detail)
            .collect()
    }

    /// Name of the index backing `table`'s primary key.
    pub fn pk_index(table: &str) -> String {
        #[cfg(feature = "sqlite")]
        return format!("sqlite_autoindex_{table}_1");
        #[cfg(not(feature = "sqlite"))]
        return format!("{table}_pkey");
    }

    /// [`plan_tx`] that also disables sorts on Postgres, which at test size
    /// prefers a bitmap scan and a sort over walking an ordered index.
    pub async fn ordered_plan_tx(
        pool: &DbPool,
        tables: &[&str],
    ) -> sqlx::Transaction<'static, db::Db> {
        #[allow(unused_mut)]
        let mut tx = plan_tx(pool, tables).await;
        #[cfg(not(feature = "sqlite"))]
        sqlx::query("SET LOCAL enable_sort = off")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx
    }

    /// virt1-shaped `sqlite_stat1` rows: ~2.34M events over 30 projects, ~200k
    /// per item type; ~13.9M spans, ~6 per trace; ~6k issues with ~400k tag rows.
    #[cfg(feature = "sqlite")]
    pub const VIRT1_STAT1: &[(&str, &str, &str)] = &[
        ("events", "sqlite_autoindex_events_1", "2340000 1"),
        ("events", "idx_events_project_time", "2340000 78000 2 1"),
        ("events", "idx_events_type_time", "2340000 200000 2"),
        ("events", "idx_events_level", "2340000 78000 30000 2"),
        ("events", "idx_events_received", "2340000 2"),
        ("events", "idx_events_fingerprint", "2340000 2000 2"),
        ("events", "idx_events_monitor", "20000 2000 1000 1"),
        ("events", "idx_events_parent", "5000 2"),
        (
            "events",
            "idx_events_project_type_time",
            "2340000 78000 15000 2",
        ),
        ("events", "idx_events_trace", "1500000 2"),
        ("events", "idx_events_project_txn", "1200000 40000 500 2"),
        (
            "events",
            "idx_events_project_release",
            "1800000 60000 300 2 1",
        ),
        (
            "events",
            "idx_events_trace_list",
            "1500000 200000 2 1 1 1 1 1 1",
        ),
        ("events", "idx_events_txn_duration", "200000 6700 85 2 1 1"),
        ("spans", "sqlite_autoindex_spans_1", "13900000 1"),
        ("spans", "idx_spans_project_time", "13900000 463000 2"),
        ("spans", "idx_spans_trace", "13900000 6"),
        ("spans", "idx_spans_received", "13900000 2"),
        ("spans", "idx_spans_root", "2300000 1 1"),
        ("spans", "idx_spans_project_trace", "13900000 463000 6"),
        ("issues", "sqlite_autoindex_issues_1", "6000 200 1"),
        ("issues", "idx_issues_project_time", "6000 200 2"),
        ("issues", "idx_issues_project_status", "6000 200 100 2"),
        ("issues", "idx_issues_project_type", "6000 200 100 2"),
        ("issues", "idx_issues_fingerprint", "6000 1"),
        (
            "issue_tag_values",
            "sqlite_autoindex_issue_tag_values_1",
            "400000 13300 66 5 1",
        ),
        (
            "issue_tag_values",
            "idx_issue_tag_values_project_fp_key_count",
            "400000 13300 66 5 2",
        ),
        (
            "issue_tag_values",
            "idx_itv_key_value",
            "400000 13300 900 20 1",
        ),
    ];

    /// Replace the planner statistics of every table named in `rows` with the given
    /// `(table, index, stat)` rows, so plans reflect production row counts.
    #[cfg(feature = "sqlite")]
    pub async fn seed_sqlite_stat1(pool: &DbPool, rows: &[(&str, &str, &str)]) {
        let mut conn = pool.acquire().await.unwrap();
        let mut tables: Vec<&str> = rows.iter().map(|(table, _, _)| *table).collect();
        tables.sort_unstable();
        tables.dedup();
        for table in tables {
            // ANALYZE creates the stat tables; its rows for `table` are then replaced.
            sqlx::query(db::dyn_sql(&format!("ANALYZE \"{table}\"")))
                .execute(&mut *conn)
                .await
                .unwrap();
            // STAT4 samples take precedence over stat1 estimates.
            for stat_table in ["sqlite_stat1", "sqlite_stat4"] {
                sqlx::query(db::dyn_sql(&format!(
                    "DELETE FROM {stat_table} WHERE tbl = ?1"
                )))
                .bind(table)
                .execute(&mut *conn)
                .await
                .unwrap();
            }
        }
        for (table, index, stat) in rows {
            sqlx::query("INSERT INTO sqlite_stat1 (tbl, idx, stat) VALUES (?1, ?2, ?3)")
                .bind(table)
                .bind(index)
                .bind(stat)
                .execute(&mut *conn)
                .await
                .unwrap();
        }
        sqlx::query("ANALYZE sqlite_schema")
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    /// A transaction to explain in: virt1 stats on SQLite; on Postgres `tables`
    /// vacuum-analyzed and sequential scans disabled.
    pub async fn plan_tx(pool: &DbPool, tables: &[&str]) -> sqlx::Transaction<'static, db::Db> {
        #[cfg(feature = "sqlite")]
        {
            let _ = tables;
            seed_sqlite_stat1(pool, VIRT1_STAT1).await;
            pool.begin().await.unwrap()
        }
        #[cfg(not(feature = "sqlite"))]
        {
            for table in tables {
                sqlx::query(db::dyn_sql(&format!("VACUUM ANALYZE {table}")))
                    .execute(pool)
                    .await
                    .unwrap();
            }
            let mut tx = pool.begin().await.unwrap();
            sqlx::query("SET LOCAL enable_seqscan = off")
                .execute(&mut *tx)
                .await
                .unwrap();
            tx
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The sniff decides whether a search term is treated as a trace id at all,
    // so it has to stay narrow enough that ordinary words fall through to the
    // title search.
    #[test]
    fn only_trace_shaped_terms_are_read_as_trace_ids() {
        let full = "0123456789abcdef0123456789abcdef";
        assert_eq!(trace_id_candidate(full).as_deref(), Some(full));
        assert_eq!(
            trace_id_candidate(&format!("  {} ", full.to_uppercase())).as_deref(),
            Some(full),
            "pasted ids arrive padded and in either case"
        );
        assert_eq!(
            trace_id_candidate("0123456789ab").as_deref(),
            Some("0123456789ab")
        );

        assert!(trace_id_candidate("deadbee").is_none(), "too short");
        assert!(
            trace_id_candidate(&format!("{full}0")).is_none(),
            "too long"
        );
        assert!(trace_id_candidate("TypeError").is_none());
        assert!(trace_id_candidate("timeout!").is_none());
        assert!(trace_id_candidate("").is_none());
    }

    #[test]
    fn like_patterns_escape_their_metacharacters() {
        assert_eq!(like_contains("100%_x"), "%100\\%\\_x%");
        assert_eq!(like_prefix("a\\b"), "a\\\\b%");
    }
}
