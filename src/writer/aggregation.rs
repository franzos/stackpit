use crate::queries::event_writes;
use anyhow::Result;
use sqlx::QueryBuilder;
use std::collections::HashMap;

use super::accumulator::Accumulators;
use super::alerting::{ThresholdCandidate, TRIGGER_CHUNK_SIZE};

/// Stored (users_hll, users_crashed_hll) blobs for a session-aggregate row.
type HllPair = (Option<Vec<u8>>, Option<Vec<u8>>);

/// Max issues per multi-row INSERT chunk. 10 bind params per issue;
/// 32766 / 10 = 3276, use 3200 for margin.
const ISSUE_UPSERT_CHUNK_SIZE: usize = 3200;

/// The sketch to bind for a row: the stored registers merged with this flush's,
/// or `None` when nothing changed so the upsert's COALESCE leaves the row alone.
fn merged_sketch(stored: Option<&[u8]>, fresh: &simple_hll::HyperLogLog<12>) -> Option<Vec<u8>> {
    use crate::ingest::models::HLL_REGISTER_COUNT;
    match stored {
        Some(buf) if buf.len() == HLL_REGISTER_COUNT => {
            let mut base = simple_hll::HyperLogLog::<12>::with_registers(buf.to_vec());
            base.merge(fresh);
            let merged = base.get_registers().to_vec();
            (merged != buf).then_some(merged)
        }
        _ => Some(fresh.get_registers().to_vec()),
    }
}

/// Batch-fetch existing issue statuses for a set of `(project_id, fingerprint)` keys.
///
/// Keys not present in the map don't exist in the issues table yet.
async fn detect_existing_issue_statuses(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    keys: &[(i64, String)],
) -> Result<HashMap<(u64, String), String>> {
    use sqlx::Row;

    let mut statuses = HashMap::with_capacity(keys.len());
    if keys.is_empty() {
        return Ok(statuses);
    }

    // Two binds per key.
    for chunk in keys.chunks(TRIGGER_CHUNK_SIZE / 2) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "SELECT project_id, fingerprint, status FROM issues WHERE ",
        );
        crate::queries::retention::push_pair_in_list(&mut builder, chunk);

        // Propagate so a transient DB error aborts the tx (and retries) instead of returning a partial map that misreads existing issues as new.
        let rows = builder.build().fetch_all(&mut **tx).await?;

        for row in &rows {
            let project_id: i64 = row.get("project_id");
            let fp: String = row.get("fingerprint");
            let status: String = row.get("status");
            statuses.insert((project_id as u64, fp), status);
        }
    }

    Ok(statuses)
}

/// The actual aggregation logic inside a transaction.
///
/// Returns threshold-check candidates so the caller can run them outside the TX.
pub(super) async fn flush_aggregation_inner(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    accumulators: &Accumulators,
    pending: &mut Vec<crate::notify::NotificationEvent>,
) -> Result<Vec<ThresholdCandidate>> {
    use crate::ingest::models::HLL_REGISTER_COUNT;
    use sqlx::Row;

    let issue_count = accumulators.issues.len();
    let tag_count = accumulators.tags.len();
    let mut threshold_candidates = Vec::new();

    let issue_keys: Vec<(i64, String)> = accumulators
        .issues
        .keys()
        .map(|(pid, fp)| (*pid as i64, fp.clone()))
        .collect();
    let existing_statuses = detect_existing_issue_statuses(tx, &issue_keys).await?;

    let now = chrono::Utc::now().timestamp();

    for (key, delta) in &accumulators.issues {
        let fingerprint = &key.1;
        match existing_statuses.get(key) {
            None => {
                pending.push(crate::notify::NotificationEvent {
                    trigger: crate::notify::NotifyTrigger::NewIssue,
                    project_id: delta.project_id,
                    fingerprint: fingerprint.clone(),
                    title: delta.title.clone(),
                    level: delta.level.map(|l| l.to_string()),
                    environment: delta.environments.iter().next().cloned(),
                    environments: delta.environments.iter().cloned().collect(),
                    event_id: String::new(),
                    digest: None,
                });
            }
            Some(status) if status == "resolved" => {
                pending.push(crate::notify::NotificationEvent {
                    trigger: crate::notify::NotifyTrigger::Regression,
                    project_id: delta.project_id,
                    fingerprint: fingerprint.clone(),
                    title: delta.title.clone(),
                    level: delta.level.map(|l| l.to_string()),
                    environment: delta.environments.iter().next().cloned(),
                    environments: delta.environments.iter().cloned().collect(),
                    event_id: String::new(),
                    digest: None,
                });
            }
            _ => {
                // existing, not resolved: candidate for threshold alerts (post-TX)
                threshold_candidates.push(ThresholdCandidate {
                    fingerprint: fingerprint.clone(),
                    project_id: delta.project_id,
                    title: delta.title.clone(),
                    level: delta.level.map(|l| l.to_string()),
                    environments: delta.environments.iter().cloned().collect(),
                });
            }
        }
    }

    // Stored sketches for the issues carrying user data this flush, read in
    // batches so the merged blob can ride the upsert below.
    let mut hll_keys: Vec<(i64, String)> = accumulators
        .issues
        .iter()
        .filter(|(_, d)| d.has_hll_data)
        .map(|((pid, fp), _)| (*pid as i64, fp.clone()))
        .collect();
    hll_keys.sort_unstable();
    let mut existing_hlls: HashMap<(u64, String), Vec<u8>> = HashMap::new();
    for chunk in hll_keys.chunks(TRIGGER_CHUNK_SIZE / 2) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "SELECT project_id, fingerprint, user_hll FROM issues WHERE user_hll IS NOT NULL AND ",
        );
        crate::queries::retention::push_pair_in_list(&mut builder, chunk);

        let hll_rows = builder.build().fetch_all(&mut **tx).await?;
        for row in &hll_rows {
            let project_id: i64 = row.get("project_id");
            let fp: String = row.get("fingerprint");
            let hll_data: Option<Vec<u8>> = row.get("user_hll");
            if let Some(data) = hll_data {
                if data.len() == HLL_REGISTER_COUNT {
                    existing_hlls.insert((project_id as u64, fp), data);
                }
            }
        }
    }

    struct IssueRow<'a> {
        fingerprint: &'a str,
        project_id: i64,
        title: Option<&'a str>,
        level: Option<&'a str>,
        first_seen: i64,
        last_seen: i64,
        event_count: i64,
        item_type: &'a str,
        user_hll: Option<Vec<u8>>,
    }

    let mut rows: Vec<IssueRow<'_>> = Vec::with_capacity(accumulators.issues.len());
    for (key, delta) in &accumulators.issues {
        let first_seen = if delta.first_seen == i64::MAX {
            now
        } else {
            delta.first_seen
        };
        let last_seen = if delta.last_seen == i64::MIN {
            now
        } else {
            delta.last_seen
        };
        let user_hll = if delta.has_hll_data {
            merged_sketch(existing_hlls.get(key).map(Vec::as_slice), &delta.hll)
        } else {
            None
        };
        rows.push(IssueRow {
            fingerprint: &key.1,
            project_id: delta.project_id as i64,
            title: delta.title.as_deref(),
            level: delta.level.as_ref().map(|l| l.as_str()),
            first_seen,
            last_seen,
            event_count: delta.event_count as i64,
            item_type: &delta.item_type,
            user_hll,
        });
    }
    // Deterministic key order so concurrent writers acquire row locks in the same order.
    rows.sort_unstable_by(|a, b| (a.project_id, a.fingerprint).cmp(&(b.project_id, b.fingerprint)));

    for chunk in rows.chunks(ISSUE_UPSERT_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "INSERT INTO issues (fingerprint, project_id, title, level, first_seen, last_seen, event_count, status, item_type, user_hll) ",
        );

        builder.push_values(chunk.iter(), |mut b, row| {
            b.push_bind(row.fingerprint);
            b.push_bind(row.project_id);
            b.push_bind(row.title);
            b.push_bind(row.level);
            b.push_bind(row.first_seen);
            b.push_bind(row.last_seen);
            b.push_bind(row.event_count);
            b.push_bind("unresolved");
            b.push_bind(row.item_type);
            b.push_bind(row.user_hll.clone());
        });

        // A NULL sketch means "unchanged this flush": COALESCE keeps the stored one.
        #[cfg(feature = "sqlite")]
        builder.push(
            " ON CONFLICT(project_id, fingerprint) DO UPDATE SET \
                 first_seen = MIN(issues.first_seen, excluded.first_seen), \
                 last_seen = MAX(issues.last_seen, excluded.last_seen), \
                 event_count = issues.event_count + excluded.event_count, \
                 title = COALESCE(excluded.title, issues.title), \
                 level = COALESCE(excluded.level, issues.level), \
                 status = CASE WHEN issues.status = 'resolved' THEN 'unresolved' ELSE issues.status END, \
                 user_hll = COALESCE(excluded.user_hll, issues.user_hll)",
        );
        #[cfg(not(feature = "sqlite"))]
        builder.push(
            " ON CONFLICT(project_id, fingerprint) DO UPDATE SET \
                 first_seen = LEAST(issues.first_seen, excluded.first_seen), \
                 last_seen = GREATEST(issues.last_seen, excluded.last_seen), \
                 event_count = issues.event_count + excluded.event_count, \
                 title = COALESCE(excluded.title, issues.title), \
                 level = COALESCE(excluded.level, issues.level), \
                 status = CASE WHEN issues.status = 'resolved' THEN 'unresolved' ELSE issues.status END, \
                 user_hll = COALESCE(excluded.user_hll, issues.user_hll)",
        );

        builder.build().execute(&mut **tx).await?;
    }

    event_writes::bulk_upsert_tag_counts(tx, &accumulators.tags).await?;
    flush_session_aggregates(tx, accumulators).await?;
    flush_transaction_metrics(tx, accumulators).await?;
    flush_releases(tx, accumulators).await?;

    tracing::debug!("aggregation flush: {issue_count} issues, {tag_count} tag entries");
    Ok(threshold_candidates)
}

/// Max release rows per multi-row INSERT chunk. 5 bind params per row;
/// 32766 / 5 = 6553, use 6400 for margin.
const RELEASE_UPSERT_CHUNK_SIZE: usize = 6400;

/// Materialize releases seen on events. Keeps `releases` the single source of
/// truth, so versions that were only ever ingested sit alongside the ones
/// registered up front by sync or a sourcemap upload.
async fn flush_releases(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    accumulators: &Accumulators,
) -> Result<()> {
    if accumulators.releases.is_empty() {
        return Ok(());
    }

    let mut rows: Vec<(i64, &str, i64, i64)> = accumulators
        .releases
        .iter()
        .map(|((project_id, version), delta)| {
            (
                *project_id as i64,
                version.as_str(),
                delta.first_seen,
                delta.last_seen,
            )
        })
        .collect();
    // Deterministic key order so concurrent writers acquire row locks in the same order.
    rows.sort_unstable_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));

    for chunk in rows.chunks(RELEASE_UPSERT_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "INSERT INTO releases (project_id, version, first_event, last_event, version_sort) ",
        );

        builder.push_values(chunk.iter(), |mut b, row| {
            b.push_bind(row.0);
            b.push_bind(row.1);
            b.push_bind(row.2);
            b.push_bind(row.3);
            b.push_bind(crate::util::version::version_sort_key(row.1));
        });

        // first_event/last_event are nullable (a release registered via the
        // sourcemap API has neither), and sqlite's scalar MIN/MAX return NULL if
        // any argument is NULL. COALESCE on both sides makes this behave exactly
        // like postgres LEAST/GREATEST, which skip NULLs on their own.
        #[cfg(feature = "sqlite")]
        builder.push(
            " ON CONFLICT(project_id, version) DO UPDATE SET \
                 first_event = MIN(COALESCE(releases.first_event, excluded.first_event), COALESCE(excluded.first_event, releases.first_event)), \
                 last_event = MAX(COALESCE(releases.last_event, excluded.last_event), COALESCE(excluded.last_event, releases.last_event)), \
                 version_sort = excluded.version_sort",
        );
        #[cfg(not(feature = "sqlite"))]
        builder.push(
            " ON CONFLICT(project_id, version) DO UPDATE SET \
                 first_event = LEAST(releases.first_event, excluded.first_event), \
                 last_event = GREATEST(releases.last_event, excluded.last_event), \
                 version_sort = excluded.version_sort",
        );

        builder.build().execute(&mut **tx).await?;
    }

    Ok(())
}

/// Max session-aggregate rows per multi-row INSERT chunk. 13 bind params per
/// row; 32766 / 13 = 2520, use 2400 for margin.
const SESSION_UPSERT_CHUNK_SIZE: usize = 2400;

/// Session-aggregate keys per batched sketch read: 4 binds per key.
const SESSION_HLL_READ_CHUNK: usize = 4000;

/// UPSERT session rollups, merging their user HLL sketches in the same statement.
async fn flush_session_aggregates(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    accumulators: &Accumulators,
) -> Result<()> {
    use sqlx::Row;

    if accumulators.session_aggregates.is_empty() {
        return Ok(());
    }

    let now = chrono::Utc::now().timestamp();

    // Stored sketches for the keys carrying user data this flush.
    let mut hll_keys: Vec<&(u64, String, String, i64)> = accumulators
        .session_aggregates
        .iter()
        .filter(|(_, d)| d.has_user_data)
        .map(|(k, _)| k)
        .collect();
    hll_keys.sort_unstable();
    let mut existing: HashMap<(u64, String, String, i64), HllPair> = HashMap::new();
    for chunk in hll_keys.chunks(SESSION_HLL_READ_CHUNK) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "SELECT project_id, release, environment, day_bucket, users_hll, users_crashed_hll \
             FROM session_aggregates WHERE (project_id, release, environment, day_bucket) IN (",
        );
        {
            let mut sep = builder.separated(", ");
            for (project_id, release, environment, day_bucket) in chunk.iter().copied() {
                sep.push("(")
                    .push_bind_unseparated(*project_id as i64)
                    .push_unseparated(", ")
                    .push_bind_unseparated(release.as_str())
                    .push_unseparated(", ")
                    .push_bind_unseparated(environment.as_str())
                    .push_unseparated(", ")
                    .push_bind_unseparated(*day_bucket)
                    .push_unseparated(")");
            }
        }
        builder.push(")");
        for row in builder.build().fetch_all(&mut **tx).await? {
            let key = (
                row.get::<i64, _>("project_id") as u64,
                row.get::<String, _>("release"),
                row.get::<String, _>("environment"),
                row.get::<i64, _>("day_bucket"),
            );
            existing.insert(key, (row.get("users_hll"), row.get("users_crashed_hll")));
        }
    }

    struct SessRow<'a> {
        project_id: i64,
        release: &'a str,
        environment: &'a str,
        day_bucket: i64,
        total: i64,
        crashed: i64,
        errored: i64,
        abnormal: i64,
        has_aggregate: i64,
        first_seen: i64,
        last_seen: i64,
        users_hll: Option<Vec<u8>>,
        users_crashed_hll: Option<Vec<u8>>,
    }

    let mut rows: Vec<SessRow<'_>> = Vec::with_capacity(accumulators.session_aggregates.len());
    for (key, delta) in &accumulators.session_aggregates {
        let (project_id, release, environment, day_bucket) = key;
        let first_seen = if delta.first_seen == i64::MAX {
            now
        } else {
            delta.first_seen
        };
        let last_seen = if delta.last_seen == i64::MIN {
            now
        } else {
            delta.last_seen
        };
        let (users_hll, users_crashed_hll) = if delta.has_user_data {
            let (stored_users, stored_crashed) = existing
                .get(key)
                .map(|(u, c)| (u.as_deref(), c.as_deref()))
                .unwrap_or((None, None));
            (
                merged_sketch(stored_users, &delta.users_hll),
                merged_sketch(stored_crashed, &delta.users_crashed_hll),
            )
        } else {
            (None, None)
        };
        rows.push(SessRow {
            project_id: *project_id as i64,
            release,
            environment,
            day_bucket: *day_bucket,
            total: delta.total as i64,
            crashed: delta.crashed as i64,
            errored: delta.errored as i64,
            abnormal: delta.abnormal as i64,
            has_aggregate: i64::from(delta.has_aggregate),
            first_seen,
            last_seen,
            users_hll,
            users_crashed_hll,
        });
    }
    // Deterministic key order so concurrent writers acquire row locks in the same order.
    rows.sort_unstable_by(|a, b| {
        (a.project_id, a.release, a.environment, a.day_bucket).cmp(&(
            b.project_id,
            b.release,
            b.environment,
            b.day_bucket,
        ))
    });

    for chunk in rows.chunks(SESSION_UPSERT_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "INSERT INTO session_aggregates (project_id, release, environment, day_bucket, sessions_total, sessions_crashed, sessions_errored, sessions_abnormal, has_aggregate, first_seen, last_seen, users_hll, users_crashed_hll) ",
        );

        builder.push_values(chunk.iter(), |mut b, row| {
            b.push_bind(row.project_id);
            b.push_bind(row.release);
            b.push_bind(row.environment);
            b.push_bind(row.day_bucket);
            b.push_bind(row.total);
            b.push_bind(row.crashed);
            b.push_bind(row.errored);
            b.push_bind(row.abnormal);
            b.push_bind(row.has_aggregate);
            b.push_bind(row.first_seen);
            b.push_bind(row.last_seen);
            b.push_bind(row.users_hll.clone());
            b.push_bind(row.users_crashed_hll.clone());
        });

        // A NULL sketch means "unchanged this flush": COALESCE keeps the stored one.
        #[cfg(feature = "sqlite")]
        builder.push(
            " ON CONFLICT(project_id, release, environment, day_bucket) DO UPDATE SET \
                 sessions_total = session_aggregates.sessions_total + excluded.sessions_total, \
                 sessions_crashed = session_aggregates.sessions_crashed + excluded.sessions_crashed, \
                 sessions_errored = session_aggregates.sessions_errored + excluded.sessions_errored, \
                 sessions_abnormal = session_aggregates.sessions_abnormal + excluded.sessions_abnormal, \
                 has_aggregate = MAX(session_aggregates.has_aggregate, excluded.has_aggregate), \
                 first_seen = MIN(session_aggregates.first_seen, excluded.first_seen), \
                 last_seen = MAX(session_aggregates.last_seen, excluded.last_seen), \
                 users_hll = COALESCE(excluded.users_hll, session_aggregates.users_hll), \
                 users_crashed_hll = COALESCE(excluded.users_crashed_hll, session_aggregates.users_crashed_hll)",
        );
        #[cfg(not(feature = "sqlite"))]
        builder.push(
            " ON CONFLICT(project_id, release, environment, day_bucket) DO UPDATE SET \
                 sessions_total = session_aggregates.sessions_total + excluded.sessions_total, \
                 sessions_crashed = session_aggregates.sessions_crashed + excluded.sessions_crashed, \
                 sessions_errored = session_aggregates.sessions_errored + excluded.sessions_errored, \
                 sessions_abnormal = session_aggregates.sessions_abnormal + excluded.sessions_abnormal, \
                 has_aggregate = GREATEST(session_aggregates.has_aggregate, excluded.has_aggregate), \
                 first_seen = LEAST(session_aggregates.first_seen, excluded.first_seen), \
                 last_seen = GREATEST(session_aggregates.last_seen, excluded.last_seen), \
                 users_hll = COALESCE(excluded.users_hll, session_aggregates.users_hll), \
                 users_crashed_hll = COALESCE(excluded.users_crashed_hll, session_aggregates.users_crashed_hll)",
        );

        builder.build().execute(&mut **tx).await?;
    }

    Ok(())
}

/// Max transaction-metric rows per multi-row INSERT chunk. 33 bind params per
/// row (3 key + count/sum/failed + 24 buckets + 2 seen + sketch); 32766 / 33 = 992,
/// use 950 for margin.
const TXN_UPSERT_CHUNK_SIZE: usize = 950;

/// Transaction-metric keys per batched sketch read: 3 binds per key.
const TXN_HLL_READ_CHUNK: usize = 5000;

/// Column list for the histogram buckets, used by both INSERT and the
/// `existing + excluded` UPDATE clause. Keep in lockstep with the migration.
const TXN_BUCKET_COLS: &str = "bucket_0, bucket_1, bucket_2, bucket_3, bucket_4, bucket_5, bucket_6, bucket_7, bucket_8, bucket_9, bucket_10, bucket_11, bucket_12, bucket_13, bucket_14, bucket_15, bucket_16, bucket_17, bucket_18, bucket_19, bucket_20, bucket_21, bucket_22, bucket_23";

/// UPSERT transaction perf rollups, merging their user HLL sketches in the same statement.
async fn flush_transaction_metrics(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    accumulators: &Accumulators,
) -> Result<()> {
    use sqlx::Row;

    if accumulators.transaction_metrics.is_empty() {
        return Ok(());
    }

    let now = chrono::Utc::now().timestamp();

    // Stored sketches for the keys carrying user data this flush.
    let mut hll_keys: Vec<&(u64, String, i64)> = accumulators
        .transaction_metrics
        .iter()
        .filter(|(_, d)| d.has_user_data)
        .map(|(k, _)| k)
        .collect();
    hll_keys.sort_unstable();
    let mut existing: HashMap<(u64, String, i64), Vec<u8>> = HashMap::new();
    for chunk in hll_keys.chunks(TXN_HLL_READ_CHUNK) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "SELECT project_id, transaction_name, hour_bucket, users_hll FROM transaction_metrics \
             WHERE users_hll IS NOT NULL AND (project_id, transaction_name, hour_bucket) IN (",
        );
        {
            let mut sep = builder.separated(", ");
            for (project_id, name, hour_bucket) in chunk.iter().copied() {
                sep.push("(")
                    .push_bind_unseparated(*project_id as i64)
                    .push_unseparated(", ")
                    .push_bind_unseparated(name.as_str())
                    .push_unseparated(", ")
                    .push_bind_unseparated(*hour_bucket)
                    .push_unseparated(")");
            }
        }
        builder.push(")");
        for row in builder.build().fetch_all(&mut **tx).await? {
            let key = (
                row.get::<i64, _>("project_id") as u64,
                row.get::<String, _>("transaction_name"),
                row.get::<i64, _>("hour_bucket"),
            );
            existing.insert(key, row.get("users_hll"));
        }
    }

    struct TxnRow<'a> {
        project_id: i64,
        name: &'a str,
        hour_bucket: i64,
        count: i64,
        sum_duration_ms: i64,
        failed_count: i64,
        buckets: [i64; 24],
        first_seen: i64,
        last_seen: i64,
        users_hll: Option<Vec<u8>>,
    }

    let mut rows: Vec<TxnRow<'_>> = Vec::with_capacity(accumulators.transaction_metrics.len());
    for (key, delta) in &accumulators.transaction_metrics {
        let (project_id, name, hour_bucket) = key;
        let first_seen = if delta.first_seen == i64::MAX {
            now
        } else {
            delta.first_seen
        };
        let last_seen = if delta.last_seen == i64::MIN {
            now
        } else {
            delta.last_seen
        };
        let mut buckets = [0i64; 24];
        for (i, b) in delta.buckets.iter().enumerate() {
            buckets[i] = *b as i64;
        }
        let users_hll = if delta.has_user_data {
            merged_sketch(existing.get(key).map(Vec::as_slice), &delta.users_hll)
        } else {
            None
        };
        rows.push(TxnRow {
            project_id: *project_id as i64,
            name,
            hour_bucket: *hour_bucket,
            count: delta.count as i64,
            sum_duration_ms: delta.sum_duration_ms as i64,
            failed_count: delta.failed_count as i64,
            buckets,
            first_seen,
            last_seen,
            users_hll,
        });
    }
    // Deterministic key order so concurrent writers acquire row locks in the same order.
    rows.sort_unstable_by(|a, b| {
        (a.project_id, a.name, a.hour_bucket).cmp(&(b.project_id, b.name, b.hour_bucket))
    });

    // Build the "col = table.col + excluded.col" list for the 24 buckets once.
    let bucket_updates: String = (0..24)
        .map(|i| format!("bucket_{i} = transaction_metrics.bucket_{i} + excluded.bucket_{i}"))
        .collect::<Vec<_>>()
        .join(", ");

    #[cfg(feature = "sqlite")]
    let (min_fn, max_fn) = ("MIN", "MAX");
    #[cfg(not(feature = "sqlite"))]
    let (min_fn, max_fn) = ("LEAST", "GREATEST");

    // A NULL sketch means "unchanged this flush": COALESCE keeps the stored one.
    let conflict_clause = format!(
        " ON CONFLICT(project_id, transaction_name, hour_bucket) DO UPDATE SET \
             count = transaction_metrics.count + excluded.count, \
             sum_duration_ms = transaction_metrics.sum_duration_ms + excluded.sum_duration_ms, \
             failed_count = transaction_metrics.failed_count + excluded.failed_count, \
             {bucket_updates}, \
             first_seen = {min_fn}(transaction_metrics.first_seen, excluded.first_seen), \
             last_seen = {max_fn}(transaction_metrics.last_seen, excluded.last_seen), \
             users_hll = COALESCE(excluded.users_hll, transaction_metrics.users_hll)"
    );

    let insert_prefix = format!(
        "INSERT INTO transaction_metrics (project_id, transaction_name, hour_bucket, count, sum_duration_ms, failed_count, {TXN_BUCKET_COLS}, first_seen, last_seen, users_hll) "
    );

    for chunk in rows.chunks(TXN_UPSERT_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(&insert_prefix);

        builder.push_values(chunk.iter(), |mut b, row| {
            b.push_bind(row.project_id);
            b.push_bind(row.name);
            b.push_bind(row.hour_bucket);
            b.push_bind(row.count);
            b.push_bind(row.sum_duration_ms);
            b.push_bind(row.failed_count);
            for bucket in &row.buckets {
                b.push_bind(*bucket);
            }
            b.push_bind(row.first_seen);
            b.push_bind(row.last_seen);
            b.push_bind(row.users_hll.clone());
        });

        builder.push(&conflict_clause);
        builder.build().execute(&mut **tx).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::accumulator::Accumulators;
    use super::super::flush::{flush_aggregation, insert_event};
    use crate::ingest::models::{ItemType, SessionBucket, StorableEvent};
    use simple_hll::HyperLogLog;
    use sqlx::Row;

    fn session_event(event_id: &str, did: &str, crashed: u64) -> StorableEvent {
        let mut e = StorableEvent::new(
            event_id.to_string(),
            ItemType::Session,
            vec![0],
            1,
            "k".to_string(),
        );
        e.timestamp = 1000;
        e.release = Some("app@1.0".to_string());
        e.session_buckets = vec![SessionBucket {
            release: "app@1.0".to_string(),
            environment: "prod".to_string(),
            started_ts: 1000,
            total: 1,
            crashed,
            errored: 0,
            abnormal: 0,
            did: Some(did.to_string()),
            is_aggregate: false,
        }];
        e
    }

    fn txn_event(event_id: &str, user: &str) -> StorableEvent {
        let mut e = StorableEvent::new(
            event_id.to_string(),
            ItemType::Transaction,
            vec![0],
            1,
            "k".to_string(),
        );
        e.timestamp = 1000;
        e.transaction_name = Some("/api/x".to_string());
        e.duration_ms = Some(100);
        e.user_identifier = Some(user.to_string());
        e
    }

    fn issue_event(event_id: &str, user: &str) -> StorableEvent {
        let mut e = StorableEvent::test_default(event_id);
        e.fingerprint = Some("hll_fp".to_string());
        e.user_identifier = Some(user.to_string());
        e
    }

    async fn flush(pool: &crate::db::DbPool, events: &[StorableEvent]) {
        let mut acc = Accumulators::new();
        for e in events {
            insert_event(pool, e).await.unwrap();
            acc.accumulate(e);
        }
        flush_aggregation(pool, &mut acc, None).await.unwrap();
    }

    fn count(blob: Option<Vec<u8>>) -> u64 {
        HyperLogLog::<12>::with_registers(blob.expect("sketch stored")).count() as u64
    }

    /// The stored sketch must be merged with, not replaced by, the next flush's.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn session_hll_merges_across_two_flushes() {
        let pool = crate::db::open_test_pool().await;
        flush(
            &pool,
            &[session_event("s1", "u1", 0), session_event("s2", "u2", 1)],
        )
        .await;
        flush(&pool, &[session_event("s3", "u3", 0)]).await;

        let row = sqlx::query(
            "SELECT sessions_total, users_hll, users_crashed_hll FROM session_aggregates \
             WHERE project_id = 1 AND release = 'app@1.0' AND environment = 'prod'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("sessions_total"), 3);
        assert_eq!(count(row.get("users_hll")), 3);
        assert_eq!(count(row.get("users_crashed_hll")), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn transaction_hll_merges_across_two_flushes() {
        let pool = crate::db::open_test_pool().await;
        flush(&pool, &[txn_event("t1", "u1"), txn_event("t2", "u2")]).await;
        flush(&pool, &[txn_event("t3", "u3")]).await;

        let row = sqlx::query(
            "SELECT count, users_hll FROM transaction_metrics \
             WHERE project_id = 1 AND transaction_name = '/api/x'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("count"), 3);
        assert_eq!(count(row.get("users_hll")), 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn issue_hll_merges_across_two_flushes() {
        let pool = crate::db::open_test_pool().await;
        flush(&pool, &[issue_event("i1", "u1"), issue_event("i2", "u2")]).await;
        flush(&pool, &[issue_event("i3", "u3")]).await;
        // A flush without user data must leave the stored sketch alone.
        let mut no_user = StorableEvent::test_default("i4");
        no_user.fingerprint = Some("hll_fp".to_string());
        flush(&pool, &[no_user]).await;

        let row = sqlx::query(
            "SELECT event_count, user_hll FROM issues WHERE project_id = 1 AND fingerprint = 'hll_fp'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("event_count"), 4);
        assert_eq!(count(row.get("user_hll")), 3);
    }
}
