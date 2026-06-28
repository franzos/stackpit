use crate::db::{sql, translate_sql};
use crate::queries::event_writes;
use anyhow::Result;
use sqlx::QueryBuilder;
use std::collections::HashMap;

use super::accumulator::Accumulators;
use super::alerting::{ThresholdCandidate, TRIGGER_CHUNK_SIZE};

/// Stored (users_hll, users_crashed_hll) blobs for a session-aggregate row.
type HllPair = (Option<Vec<u8>>, Option<Vec<u8>>);

/// Max issues per multi-row INSERT chunk. 9 bind params per issue;
/// 32766 / 9 = 3640, use 3600 for margin.
const ISSUE_UPSERT_CHUNK_SIZE: usize = 3600;

/// Batch-fetch existing issue statuses for a set of fingerprints.
///
/// Returns a map of fingerprint -> status string. Fingerprints not present
/// in the map don't exist in the issues table yet.
async fn detect_existing_issue_statuses(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    fingerprints: &[&str],
) -> HashMap<String, String> {
    use sqlx::Row;

    let mut statuses = HashMap::with_capacity(fingerprints.len());
    if fingerprints.is_empty() {
        return statuses;
    }

    for chunk in fingerprints.chunks(TRIGGER_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "SELECT fingerprint, status FROM issues WHERE fingerprint IN (",
        );
        let mut sep = builder.separated(", ");
        for fp in chunk {
            sep.push_bind(*fp);
        }
        sep.push_unseparated(")");

        let rows = match builder.build().fetch_all(&mut **tx).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("detect_existing_issue_statuses failed: {e}");
                return statuses;
            }
        };

        for row in &rows {
            let fp: String = row.get("fingerprint");
            let status: String = row.get("status");
            statuses.insert(fp, status);
        }
    }

    statuses
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
    use simple_hll::HyperLogLog;
    use sqlx::Row;

    let issue_count = accumulators.issues.len();
    let tag_count = accumulators.tags.len();
    let mut threshold_candidates = Vec::new();

    let fingerprints: Vec<&str> = accumulators.issues.keys().map(|s| s.as_str()).collect();
    let existing_statuses = detect_existing_issue_statuses(tx, &fingerprints).await;

    let now = chrono::Utc::now().timestamp();

    for (fingerprint, delta) in &accumulators.issues {
        match existing_statuses.get(fingerprint.as_str()) {
            None => {
                pending.push(crate::notify::NotificationEvent {
                    trigger: crate::notify::NotifyTrigger::NewIssue,
                    project_id: delta.project_id,
                    fingerprint: fingerprint.clone(),
                    title: delta.title.clone(),
                    level: delta.level.map(|l| l.to_string()),
                    environment: None,
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
                    environment: None,
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
                });
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
    }

    let mut rows: Vec<IssueRow<'_>> = Vec::with_capacity(accumulators.issues.len());
    for (fingerprint, delta) in &accumulators.issues {
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
        rows.push(IssueRow {
            fingerprint,
            project_id: delta.project_id as i64,
            title: delta.title.as_deref(),
            level: delta.level.as_ref().map(|l| l.as_str()),
            first_seen,
            last_seen,
            event_count: delta.event_count as i64,
            item_type: &delta.item_type,
        });
    }

    for chunk in rows.chunks(ISSUE_UPSERT_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "INSERT INTO issues (fingerprint, project_id, title, level, first_seen, last_seen, event_count, status, item_type) ",
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
        });

        #[cfg(feature = "sqlite")]
        builder.push(
            " ON CONFLICT(fingerprint) DO UPDATE SET \
                 first_seen = MIN(issues.first_seen, excluded.first_seen), \
                 last_seen = MAX(issues.last_seen, excluded.last_seen), \
                 event_count = issues.event_count + excluded.event_count, \
                 title = COALESCE(excluded.title, issues.title), \
                 level = COALESCE(excluded.level, issues.level), \
                 status = CASE WHEN issues.status = 'resolved' THEN 'unresolved' ELSE issues.status END",
        );
        #[cfg(not(feature = "sqlite"))]
        builder.push(
            " ON CONFLICT(fingerprint) DO UPDATE SET \
                 first_seen = LEAST(issues.first_seen, excluded.first_seen), \
                 last_seen = GREATEST(issues.last_seen, excluded.last_seen), \
                 event_count = issues.event_count + excluded.event_count, \
                 title = COALESCE(excluded.title, issues.title), \
                 level = COALESCE(excluded.level, issues.level), \
                 status = CASE WHEN issues.status = 'resolved' THEN 'unresolved' ELSE issues.status END",
        );

        builder.build().execute(&mut **tx).await?;
    }

    let hll_fingerprints: Vec<&str> = accumulators
        .issues
        .iter()
        .filter(|(_, d)| d.has_hll_data)
        .map(|(fp, _)| fp.as_str())
        .collect();

    if !hll_fingerprints.is_empty() {
        let mut existing_hlls: HashMap<String, Vec<u8>> = HashMap::new();
        for chunk in hll_fingerprints.chunks(TRIGGER_CHUNK_SIZE) {
            let mut builder = QueryBuilder::<crate::db::Db>::new(
                "SELECT fingerprint, user_hll FROM issues WHERE user_hll IS NOT NULL AND fingerprint IN (",
            );
            let mut sep = builder.separated(", ");
            for fp in chunk {
                sep.push_bind(*fp);
            }
            sep.push_unseparated(")");

            let hll_rows = builder.build().fetch_all(&mut **tx).await?;
            for row in &hll_rows {
                let fp: String = row.get("fingerprint");
                let hll_data: Option<Vec<u8>> = row.get("user_hll");
                if let Some(data) = hll_data {
                    if data.len() == HLL_REGISTER_COUNT {
                        existing_hlls.insert(fp, data);
                    }
                }
            }
        }

        // write back individually: each row has unique blob data
        for fp in &hll_fingerprints {
            let delta = &accumulators.issues[*fp];
            let merged = match existing_hlls.get(*fp) {
                Some(buf) => {
                    let mut base = HyperLogLog::with_registers(buf.clone());
                    base.merge(&delta.hll);
                    base
                }
                None => delta.hll.clone(),
            };

            let sql = translate_sql("UPDATE issues SET user_hll = ?1 WHERE fingerprint = ?2");
            sqlx::query(&sql)
                .bind(merged.get_registers())
                .bind(*fp)
                .execute(&mut **tx)
                .await?;
        }
    }

    event_writes::bulk_upsert_tag_counts(tx, &accumulators.tags).await?;
    flush_session_aggregates(tx, accumulators).await?;
    flush_transaction_metrics(tx, accumulators).await?;

    tracing::debug!("aggregation flush: {issue_count} issues, {tag_count} tag entries");
    Ok(threshold_candidates)
}

/// Max session-aggregate rows per multi-row INSERT chunk. 11 bind params per
/// row; 32766 / 11 = 2978, use 2700 for margin.
const SESSION_UPSERT_CHUNK_SIZE: usize = 2700;

/// UPSERT session rollups and merge their user HLL sketches in place.
async fn flush_session_aggregates(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    accumulators: &Accumulators,
) -> Result<()> {
    use crate::ingest::models::HLL_REGISTER_COUNT;
    use simple_hll::HyperLogLog;

    if accumulators.session_aggregates.is_empty() {
        return Ok(());
    }

    let now = chrono::Utc::now().timestamp();

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
    }

    let mut rows: Vec<SessRow<'_>> = Vec::with_capacity(accumulators.session_aggregates.len());
    for ((project_id, release, environment, day_bucket), delta) in &accumulators.session_aggregates
    {
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
        });
    }

    for chunk in rows.chunks(SESSION_UPSERT_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "INSERT INTO session_aggregates (project_id, release, environment, day_bucket, sessions_total, sessions_crashed, sessions_errored, sessions_abnormal, has_aggregate, first_seen, last_seen) ",
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
        });

        #[cfg(feature = "sqlite")]
        builder.push(
            " ON CONFLICT(project_id, release, environment, day_bucket) DO UPDATE SET \
                 sessions_total = session_aggregates.sessions_total + excluded.sessions_total, \
                 sessions_crashed = session_aggregates.sessions_crashed + excluded.sessions_crashed, \
                 sessions_errored = session_aggregates.sessions_errored + excluded.sessions_errored, \
                 sessions_abnormal = session_aggregates.sessions_abnormal + excluded.sessions_abnormal, \
                 has_aggregate = MAX(session_aggregates.has_aggregate, excluded.has_aggregate), \
                 first_seen = MIN(session_aggregates.first_seen, excluded.first_seen), \
                 last_seen = MAX(session_aggregates.last_seen, excluded.last_seen)",
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
                 last_seen = GREATEST(session_aggregates.last_seen, excluded.last_seen)",
        );

        builder.build().execute(&mut **tx).await?;
    }

    // HLL read-modify-write per (project, release, environment, day) with user data.
    for ((project_id, release, environment, day_bucket), delta) in &accumulators.session_aggregates
    {
        if !delta.has_user_data {
            continue;
        }

        let existing: Option<HllPair> = sqlx::query_as(sql!(
            "SELECT users_hll, users_crashed_hll FROM session_aggregates \
             WHERE project_id = ?1 AND release = ?2 AND environment = ?3 AND day_bucket = ?4"
        ))
        .bind(*project_id as i64)
        .bind(release)
        .bind(environment)
        .bind(*day_bucket)
        .fetch_optional(&mut **tx)
        .await?;

        let merge = |existing: Option<Vec<u8>>, fresh: &HyperLogLog<12>| -> Vec<u8> {
            match existing {
                Some(buf) if buf.len() == HLL_REGISTER_COUNT => {
                    let mut base = HyperLogLog::with_registers(buf);
                    base.merge(fresh);
                    base.get_registers().to_vec()
                }
                _ => fresh.get_registers().to_vec(),
            }
        };

        let (cur_users, cur_crashed) = existing.unwrap_or((None, None));
        let users_blob = merge(cur_users, &delta.users_hll);
        let crashed_blob = merge(cur_crashed, &delta.users_crashed_hll);

        sqlx::query(sql!(
            "UPDATE session_aggregates SET users_hll = ?1, users_crashed_hll = ?2 \
             WHERE project_id = ?3 AND release = ?4 AND environment = ?5 AND day_bucket = ?6"
        ))
        .bind(users_blob)
        .bind(crashed_blob)
        .bind(*project_id as i64)
        .bind(release)
        .bind(environment)
        .bind(*day_bucket)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

/// Max transaction-metric rows per multi-row INSERT chunk. 32 bind params per
/// row (3 key + count/sum/failed + 24 buckets + 2 seen); 32766 / 32 = 1023,
/// use 1000 for margin.
const TXN_UPSERT_CHUNK_SIZE: usize = 1000;

/// Column list for the histogram buckets, used by both INSERT and the
/// `existing + excluded` UPDATE clause. Keep in lockstep with the migration.
const TXN_BUCKET_COLS: &str = "bucket_0, bucket_1, bucket_2, bucket_3, bucket_4, bucket_5, bucket_6, bucket_7, bucket_8, bucket_9, bucket_10, bucket_11, bucket_12, bucket_13, bucket_14, bucket_15, bucket_16, bucket_17, bucket_18, bucket_19, bucket_20, bucket_21, bucket_22, bucket_23";

/// UPSERT transaction perf rollups and merge their user HLL sketches in place.
async fn flush_transaction_metrics(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    accumulators: &Accumulators,
) -> Result<()> {
    use crate::ingest::models::HLL_REGISTER_COUNT;
    use simple_hll::HyperLogLog;

    if accumulators.transaction_metrics.is_empty() {
        return Ok(());
    }

    let now = chrono::Utc::now().timestamp();

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
    }

    let mut rows: Vec<TxnRow<'_>> = Vec::with_capacity(accumulators.transaction_metrics.len());
    for ((project_id, name, hour_bucket), delta) in &accumulators.transaction_metrics {
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
        });
    }

    // Build the "col = table.col + excluded.col" list for the 24 buckets once.
    let bucket_updates: String = (0..24)
        .map(|i| format!("bucket_{i} = transaction_metrics.bucket_{i} + excluded.bucket_{i}"))
        .collect::<Vec<_>>()
        .join(", ");

    #[cfg(feature = "sqlite")]
    let (min_fn, max_fn) = ("MIN", "MAX");
    #[cfg(not(feature = "sqlite"))]
    let (min_fn, max_fn) = ("LEAST", "GREATEST");

    let conflict_clause = format!(
        " ON CONFLICT(project_id, transaction_name, hour_bucket) DO UPDATE SET \
             count = transaction_metrics.count + excluded.count, \
             sum_duration_ms = transaction_metrics.sum_duration_ms + excluded.sum_duration_ms, \
             failed_count = transaction_metrics.failed_count + excluded.failed_count, \
             {bucket_updates}, \
             first_seen = {min_fn}(transaction_metrics.first_seen, excluded.first_seen), \
             last_seen = {max_fn}(transaction_metrics.last_seen, excluded.last_seen)"
    );

    let insert_prefix = format!(
        "INSERT INTO transaction_metrics (project_id, transaction_name, hour_bucket, count, sum_duration_ms, failed_count, {TXN_BUCKET_COLS}, first_seen, last_seen) "
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
        });

        builder.push(&conflict_clause);
        builder.build().execute(&mut **tx).await?;
    }

    // HLL read-modify-write per (project, transaction, hour) with user data.
    for ((project_id, name, hour_bucket), delta) in &accumulators.transaction_metrics {
        if !delta.has_user_data {
            continue;
        }

        let existing: Option<(Option<Vec<u8>>,)> = sqlx::query_as(sql!(
            "SELECT users_hll FROM transaction_metrics \
             WHERE project_id = ?1 AND transaction_name = ?2 AND hour_bucket = ?3"
        ))
        .bind(*project_id as i64)
        .bind(name)
        .bind(*hour_bucket)
        .fetch_optional(&mut **tx)
        .await?;

        let merged = match existing.and_then(|(b,)| b) {
            Some(buf) if buf.len() == HLL_REGISTER_COUNT => {
                let mut base = HyperLogLog::<12>::with_registers(buf);
                base.merge(&delta.users_hll);
                base.get_registers().to_vec()
            }
            _ => delta.users_hll.get_registers().to_vec(),
        };

        sqlx::query(sql!(
            "UPDATE transaction_metrics SET users_hll = ?1 \
             WHERE project_id = ?2 AND transaction_name = ?3 AND hour_bucket = ?4"
        ))
        .bind(merged)
        .bind(*project_id as i64)
        .bind(name)
        .bind(*hour_bucket)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}
