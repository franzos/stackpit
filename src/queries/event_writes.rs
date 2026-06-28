//! Shared event write operations (writer batches for throughput; sync one-at-a-time).
//!
//! - **Writer**: `insert_event_row` → accumulators → batched issue/tag/HLL upserts
//! - **Sync CLI**: `insert_event_row` → per-event issue/tag/HLL upserts

use std::collections::HashMap;

use anyhow::Result;
use simple_hll::HyperLogLog;
use sqlx::QueryBuilder;

use crate::db::{sql, DbPool};
use crate::models::{
    ItemType, StorableAttachment, StorableEvent, HLL_REGISTER_COUNT, MAX_TAGS_PER_EVENT,
};

use super::parse_log::{
    compress_log_entry, extract_log_fields, extract_log_timestamp, parse_log_entries,
};
use super::parse_metric::parse_metric_payload;
use super::parse_span::{extract_span_fields, extract_span_fields_from_value, SpanFields};

/// Max events per multi-row INSERT chunk. 21 bind params per event;
/// SQLite's SQLITE_MAX_VARIABLE_NUMBER is 32766, so 32766 / 21 = 1560.
/// We use 1500 for a comfortable margin.
const BULK_CHUNK_SIZE: usize = 1500;

/// Max spans per multi-row INSERT chunk. 14 bind params per span;
/// 32766 / 14 = 2340, use 2300 for margin.
const SPAN_BULK_CHUNK_SIZE: usize = 2300;

/// Cap on embedded child spans extracted from a single transaction payload.
const MAX_EMBEDDED_SPANS: usize = 1000;

/// Max metrics per multi-row INSERT chunk. 8 bind params per metric;
/// 32766 / 8 = 4095, use 4000 for margin.
const METRIC_BULK_CHUNK_SIZE: usize = 4000;

/// Max logs per multi-row INSERT chunk. 11 bind params per log;
/// 32766 / 11 = 2978, use 2900 for margin.
const LOG_BULK_CHUNK_SIZE: usize = 2900;

/// Max tags per multi-row INSERT chunk. 4 bind params per tag;
/// SQLite's SQLITE_MAX_VARIABLE_NUMBER is 32766, so 32766 / 4 = 8191.
/// We use 8000 for a comfortable margin.
const TAG_CHUNK_SIZE: usize = 8000;

/// Insert an event row (INSERT OR IGNORE). Returns `true` if it was actually new.
///
/// Accepts any sqlx executor -- `&DbPool`, `&mut PgConnection`, `&mut SqliteConnection`,
/// or a dereferenced transaction (`&mut *tx`).
pub async fn insert_event_row<'e, E>(executor: E, event: &StorableEvent) -> Result<bool>
where
    E: sqlx::Executor<'e, Database = crate::db::Db>,
{
    #[cfg(feature = "sqlite")]
    let query = sql!("INSERT OR IGNORE INTO events (event_id, item_type, payload, project_id, public_key, timestamp, level, platform, release, environment, server_name, transaction_name, title, sdk_name, sdk_version, fingerprint, monitor_slug, session_status, parent_event_id, trace_id, duration_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)");
    #[cfg(not(feature = "sqlite"))]
    let query = sql!("INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, level, platform, release, environment, server_name, transaction_name, title, sdk_name, sdk_version, fingerprint, monitor_slug, session_status, parent_event_id, trace_id, duration_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
         ON CONFLICT (event_id) DO NOTHING");

    let result = sqlx::query(query)
        .bind(&event.event_id)
        .bind(event.item_type.as_str())
        .bind(&event.payload)
        .bind(event.project_id as i64)
        .bind(&event.public_key)
        .bind(event.timestamp)
        .bind(event.level.as_ref().map(|l| l.as_str()))
        .bind(&event.platform)
        .bind(&event.release)
        .bind(&event.environment)
        .bind(&event.server_name)
        .bind(&event.transaction_name)
        .bind(&event.title)
        .bind(&event.sdk_name)
        .bind(&event.sdk_version)
        .bind(&event.fingerprint)
        .bind(&event.monitor_slug)
        .bind(&event.session_status)
        .bind(&event.parent_event_id)
        .bind(&event.trace_id)
        .bind(event.duration_ms)
        .execute(executor)
        .await?;

    Ok(result.rows_affected() == 1)
}

/// Backend-specific `INSERT ... (cols) VALUES`-prefix and trailing conflict clause.
///
/// SQLite expresses idempotent inserts with a leading `INSERT OR IGNORE`;
/// Postgres uses a trailing `ON CONFLICT (...) DO NOTHING`. `conflict_col` of
/// `None` means no conflict handling at all (e.g. metrics/logs).
struct InsertIgnore {
    prefix: String,
    suffix: String,
}

fn insert_ignore(table: &str, cols: &str, conflict_col: Option<&str>) -> InsertIgnore {
    match conflict_col {
        #[cfg(feature = "sqlite")]
        Some(_) => InsertIgnore {
            prefix: format!("INSERT OR IGNORE INTO {table} ({cols}) "),
            suffix: String::new(),
        },
        #[cfg(not(feature = "sqlite"))]
        Some(col) => InsertIgnore {
            prefix: format!("INSERT INTO {table} ({cols}) "),
            suffix: format!(" ON CONFLICT ({col}) DO NOTHING"),
        },
        None => InsertIgnore {
            prefix: format!("INSERT INTO {table} ({cols}) "),
            suffix: String::new(),
        },
    }
}

/// Bulk-insert multiple event rows, routing each to the correct table.
///
/// Spans go to the spans table, metrics to the metrics table, logs to
/// the logs table, and everything else to the events table.
pub async fn insert_event_rows_bulk(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    events: &[&StorableEvent],
) -> Result<()> {
    if events.is_empty() {
        return Ok(());
    }

    let mut regular: Vec<&&StorableEvent> = Vec::new();
    let mut spans: Vec<&&StorableEvent> = Vec::new();
    let mut metrics: Vec<&&StorableEvent> = Vec::new();
    let mut logs: Vec<&&StorableEvent> = Vec::new();

    for event in events {
        match event.item_type {
            ItemType::Span => spans.push(event),
            ItemType::Metric => metrics.push(event),
            ItemType::Log => logs.push(event),
            _ => regular.push(event),
        }
    }

    if !regular.is_empty() {
        bulk_insert_events_table(tx, &regular).await?;
    }
    if !spans.is_empty() {
        bulk_insert_spans_table(tx, &spans).await?;
    }
    // Transactions carry child spans inline; pull them into the spans table so
    // the trace waterfall has rows to render.
    let embedded = extract_embedded_spans(&regular);
    if !embedded.is_empty() {
        bulk_insert_span_rows(tx, &embedded).await?;
    }
    if !metrics.is_empty() {
        bulk_insert_metrics_table(tx, &metrics).await?;
    }
    if !logs.is_empty() {
        bulk_insert_logs_table(tx, &logs).await?;
    }

    Ok(())
}

async fn bulk_insert_events_table(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    events: &[&&StorableEvent],
) -> Result<()> {
    let dialect = insert_ignore(
        "events",
        "event_id, item_type, payload, project_id, public_key, timestamp, level, platform, release, environment, server_name, transaction_name, title, sdk_name, sdk_version, fingerprint, monitor_slug, session_status, parent_event_id, trace_id, duration_ms",
        Some("event_id"),
    );

    for chunk in events.chunks(BULK_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(&dialect.prefix);

        builder.push_values(chunk.iter(), |mut b, event| {
            b.push_bind(&event.event_id);
            b.push_bind(event.item_type.as_str());
            b.push_bind(&event.payload);
            b.push_bind(event.project_id as i64);
            b.push_bind(&event.public_key);
            b.push_bind(event.timestamp);
            b.push_bind(event.level.as_ref().map(|l| l.as_str()));
            b.push_bind(&event.platform);
            b.push_bind(&event.release);
            b.push_bind(&event.environment);
            b.push_bind(&event.server_name);
            b.push_bind(&event.transaction_name);
            b.push_bind(&event.title);
            b.push_bind(&event.sdk_name);
            b.push_bind(&event.sdk_version);
            b.push_bind(&event.fingerprint);
            b.push_bind(&event.monitor_slug);
            b.push_bind(&event.session_status);
            b.push_bind(&event.parent_event_id);
            b.push_bind(&event.trace_id);
            b.push_bind(event.duration_ms);
        });

        builder.push(&dialect.suffix);

        builder.build().execute(&mut **tx).await?;
    }

    Ok(())
}

struct SpanRow {
    span_id: String,
    payload: Vec<u8>,
    project_id: i64,
    public_key: String,
    timestamp: i64,
    release: Option<String>,
    environment: Option<String>,
    trace_id: Option<String>,
    parent_span_id: Option<String>,
    op: Option<String>,
    description: Option<String>,
    status: Option<String>,
    duration_ms: Option<i64>,
    start_ms: Option<i64>,
}

/// Build a SpanRow from already-extracted fields and its source event.
/// `payload` and `trace_id_fallback` differ for standalone vs embedded spans.
fn span_row_from_fields(
    fields: SpanFields,
    event: &StorableEvent,
    payload: Vec<u8>,
    timestamp: i64,
    trace_id_fallback: Option<&str>,
    default_span_id: String,
) -> SpanRow {
    SpanRow {
        span_id: fields.span_id.unwrap_or(default_span_id),
        payload,
        project_id: event.project_id as i64,
        public_key: event.public_key.clone(),
        timestamp,
        release: event.release.clone(),
        environment: event.environment.clone(),
        trace_id: fields
            .trace_id
            .or_else(|| trace_id_fallback.map(str::to_string)),
        parent_span_id: fields.parent_span_id,
        op: fields.op,
        description: fields.description,
        status: fields.status,
        duration_ms: fields.duration_ms,
        start_ms: fields.start_ms,
    }
}

async fn bulk_insert_spans_table(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    spans: &[&&StorableEvent],
) -> Result<()> {
    let rows: Vec<SpanRow> = spans
        .iter()
        .map(|event| {
            let fields = extract_span_fields(&event.payload);
            let default_id = event.event_id.clone();
            span_row_from_fields(
                fields,
                event,
                event.payload.clone(),
                event.timestamp,
                None,
                default_id,
            )
        })
        .collect();

    bulk_insert_span_rows(tx, &rows).await
}

/// Pull child spans out of each transaction payload (capped per transaction).
/// Trace id falls back to the parent transaction's `contexts.trace.trace_id`.
fn extract_embedded_spans(events: &[&&StorableEvent]) -> Vec<SpanRow> {
    let mut rows = Vec::new();
    for event in events {
        if event.item_type != ItemType::Transaction {
            continue;
        }
        let json: Option<serde_json::Value> = zstd::decode_all(event.payload.as_slice())
            .ok()
            .or_else(|| Some(event.payload.clone()))
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());

        let Some(json) = json else { continue };
        let Some(spans) = json.get("spans").and_then(|s| s.as_array()) else {
            continue;
        };

        let parent_trace = json
            .get("contexts")
            .and_then(|c| c.get("trace"))
            .and_then(|t| t.get("trace_id"))
            .and_then(|v| v.as_str())
            .or(event.trace_id.as_deref());

        if spans.len() > MAX_EMBEDDED_SPANS {
            tracing::warn!(
                event_id = %event.event_id,
                span_count = spans.len(),
                "transaction has more than {MAX_EMBEDDED_SPANS} child spans; capping"
            );
        }

        for child in spans.iter().take(MAX_EMBEDDED_SPANS) {
            let fields = extract_span_fields_from_value(child);
            // No span_id means we can't dedupe it -- skip rather than collide
            // on the parent event_id.
            let Some(span_id) = fields.span_id.clone() else {
                continue;
            };
            let payload = serde_json::to_vec(child).unwrap_or_default();
            let ts = child
                .get("timestamp")
                .and_then(serde_json::Value::as_f64)
                .map(|f| f.round() as i64)
                .unwrap_or(event.timestamp);
            rows.push(span_row_from_fields(
                fields,
                event,
                payload,
                ts,
                parent_trace,
                span_id,
            ));
        }
    }
    rows
}

/// Shared chunked INSERT for span rows (standalone and embedded share this).
async fn bulk_insert_span_rows(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    rows: &[SpanRow],
) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }

    let dialect = insert_ignore(
        "spans",
        "span_id, payload, project_id, public_key, timestamp, release, environment, trace_id, parent_span_id, op, description, status, duration_ms, start_ms",
        Some("span_id"),
    );

    for chunk in rows.chunks(SPAN_BULK_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(&dialect.prefix);

        builder.push_values(chunk.iter(), |mut b, row| {
            b.push_bind(&row.span_id);
            b.push_bind(&row.payload);
            b.push_bind(row.project_id);
            b.push_bind(&row.public_key);
            b.push_bind(row.timestamp);
            b.push_bind(&row.release);
            b.push_bind(&row.environment);
            b.push_bind(&row.trace_id);
            b.push_bind(&row.parent_span_id);
            b.push_bind(&row.op);
            b.push_bind(&row.description);
            b.push_bind(&row.status);
            b.push_bind(row.duration_ms);
            b.push_bind(row.start_ms);
        });

        builder.push(&dialect.suffix);

        builder.build().execute(&mut **tx).await?;
    }

    Ok(())
}

async fn bulk_insert_metrics_table(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    metrics: &[&&StorableEvent],
) -> Result<()> {
    struct MetricRow {
        project_id: i64,
        public_key: String,
        timestamp: i64,
        mri: String,
        metric_type: String,
        value: f64,
        tags: Option<String>,
        values_json: Option<String>,
    }

    let rows: Vec<MetricRow> = metrics
        .iter()
        .flat_map(|event| {
            parse_metric_payload(&event.payload)
                .into_iter()
                .map(
                    |(mri, metric_type, value, tags, values_json, bucket_ts)| MetricRow {
                        project_id: event.project_id as i64,
                        public_key: event.public_key.clone(),
                        timestamp: bucket_ts.unwrap_or(event.timestamp),
                        mri,
                        metric_type,
                        value,
                        tags,
                        values_json,
                    },
                )
                .collect::<Vec<_>>()
        })
        .collect();

    let dialect = insert_ignore(
        "metrics",
        "project_id, public_key, timestamp, mri, metric_type, value, tags, \"values\"",
        None,
    );

    for chunk in rows.chunks(METRIC_BULK_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(&dialect.prefix);

        builder.push_values(chunk.iter(), |mut b, row| {
            b.push_bind(row.project_id);
            b.push_bind(&row.public_key);
            b.push_bind(row.timestamp);
            b.push_bind(&row.mri);
            b.push_bind(&row.metric_type);
            b.push_bind(row.value);
            b.push_bind(&row.tags);
            b.push_bind(&row.values_json);
        });

        builder.push(&dialect.suffix);

        builder.build().execute(&mut **tx).await?;
    }

    Ok(())
}

async fn bulk_insert_logs_table(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    logs: &[&&StorableEvent],
) -> Result<()> {
    struct LogRow {
        payload: Vec<u8>,
        project_id: i64,
        public_key: String,
        timestamp: i64,
        release: Option<String>,
        environment: Option<String>,
        trace_id: Option<String>,
        span_id: Option<String>,
        level: Option<String>,
        body: Option<String>,
        attributes: Option<String>,
    }

    let rows: Vec<LogRow> = logs
        .iter()
        .flat_map(|event| {
            let entries = parse_log_entries(&event.payload);
            entries
                .into_iter()
                .map(|entry| {
                    let fields = extract_log_fields(&entry);
                    let entry_ts = extract_log_timestamp(&entry).unwrap_or(event.timestamp);
                    let entry_payload = compress_log_entry(&entry);
                    LogRow {
                        payload: entry_payload,
                        project_id: event.project_id as i64,
                        public_key: event.public_key.clone(),
                        timestamp: entry_ts,
                        release: event.release.clone(),
                        environment: event.environment.clone(),
                        trace_id: fields.trace_id,
                        span_id: fields.span_id,
                        level: fields.level,
                        body: fields.body,
                        attributes: fields.attributes,
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();

    let dialect = insert_ignore(
        "logs",
        "payload, project_id, public_key, timestamp, release, environment, trace_id, span_id, level, body, attributes",
        None,
    );

    for chunk in rows.chunks(LOG_BULK_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(&dialect.prefix);

        builder.push_values(chunk.iter(), |mut b, row| {
            b.push_bind(&row.payload);
            b.push_bind(row.project_id);
            b.push_bind(&row.public_key);
            b.push_bind(row.timestamp);
            b.push_bind(&row.release);
            b.push_bind(&row.environment);
            b.push_bind(&row.trace_id);
            b.push_bind(&row.span_id);
            b.push_bind(&row.level);
            b.push_bind(&row.body);
            b.push_bind(&row.attributes);
        });

        builder.push(&dialect.suffix);

        builder.build().execute(&mut **tx).await?;
    }

    Ok(())
}

/// Upsert the issue row, tag counts, and HLL user counter for a new event.
/// Only call this when `insert_event_row` returned `true` and there's a fingerprint.
pub async fn upsert_issue_from_event(pool: &DbPool, event: &StorableEvent) -> Result<()> {
    let fp = match event.fingerprint.as_deref() {
        Some(fp) => fp,
        None => return Ok(()),
    };

    let mut tx = pool.begin().await?;

    #[cfg(feature = "sqlite")]
    let upsert_sql = sql!(
        "INSERT INTO issues (fingerprint, project_id, title, level, first_seen, last_seen, event_count, status, item_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'unresolved', ?8)
         ON CONFLICT(fingerprint) DO UPDATE SET
             first_seen = MIN(issues.first_seen, excluded.first_seen),
             last_seen = MAX(issues.last_seen, excluded.last_seen),
             event_count = issues.event_count + excluded.event_count,
             title = COALESCE(excluded.title, issues.title),
             level = COALESCE(excluded.level, issues.level),
             status = CASE WHEN issues.status = 'resolved' THEN 'unresolved' ELSE issues.status END"
    );
    #[cfg(not(feature = "sqlite"))]
    let upsert_sql = sql!(
        "INSERT INTO issues (fingerprint, project_id, title, level, first_seen, last_seen, event_count, status, item_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'unresolved', ?8)
         ON CONFLICT(fingerprint) DO UPDATE SET
             first_seen = LEAST(issues.first_seen, excluded.first_seen),
             last_seen = GREATEST(issues.last_seen, excluded.last_seen),
             event_count = issues.event_count + excluded.event_count,
             title = COALESCE(excluded.title, issues.title),
             level = COALESCE(excluded.level, issues.level),
             status = CASE WHEN issues.status = 'resolved' THEN 'unresolved' ELSE issues.status END"
    );
    sqlx::query(upsert_sql)
        .bind(fp)
        .bind(event.project_id as i64)
        .bind(&event.title)
        .bind(event.level.as_ref().map(|l| l.as_str()))
        .bind(event.timestamp)
        .bind(event.timestamp)
        .bind(1i64)
        .bind(event.item_type.as_str())
        .execute(&mut *tx)
        .await?;

    for (key, value) in event.tags.iter().take(MAX_TAGS_PER_EVENT) {
        sqlx::query(sql!(
            "INSERT INTO issue_tag_values (fingerprint, tag_key, tag_value, count)
             VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(fingerprint, tag_key, tag_value) DO UPDATE SET
                 count = issue_tag_values.count + 1"
        ))
        .bind(fp)
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }

    // HLL read-modify-write inside the transaction to avoid lost-update races.
    if let Some(ref user_id) = event.user_identifier {
        let existing: Option<(Vec<u8>,)> =
            sqlx::query_as(sql!("SELECT user_hll FROM issues WHERE fingerprint = ?1"))
                .bind(fp)
                .fetch_optional(&mut *tx)
                .await?;

        let mut hll: HyperLogLog<12> = match existing {
            Some((buf,)) if buf.len() == HLL_REGISTER_COUNT => HyperLogLog::with_registers(buf),
            _ => HyperLogLog::new(),
        };
        hll.add_object(user_id);

        sqlx::query(sql!(
            "UPDATE issues SET user_hll = ?1 WHERE fingerprint = ?2"
        ))
        .bind(hll.get_registers())
        .bind(fp)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Bulk-upsert accumulated tag counts using multi-row INSERT ... ON CONFLICT.
///
/// Tags are chunked to stay within the database's bind-parameter limit
/// (4 params per tag, chunks of 8000).
pub async fn bulk_upsert_tag_counts(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    tags: &HashMap<(String, String, String), u64>,
) -> Result<()> {
    if tags.is_empty() {
        return Ok(());
    }

    let entries: Vec<_> = tags.iter().collect();

    for chunk in entries.chunks(TAG_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "INSERT INTO issue_tag_values (fingerprint, tag_key, tag_value, count) ",
        );

        builder.push_values(chunk.iter(), |mut b, ((fingerprint, key, value), count)| {
            b.push_bind(fingerprint.as_str());
            b.push_bind(key.as_str());
            b.push_bind(value.as_str());
            b.push_bind(**count as i64);
        });

        builder.push(
            " ON CONFLICT(fingerprint, tag_key, tag_value) DO UPDATE SET \
             count = issue_tag_values.count + excluded.count",
        );

        builder.build().execute(&mut **tx).await?;
    }

    Ok(())
}

/// Store an attachment for an event (replaces if one with the same filename exists).
///
/// Accepts any sqlx executor -- `&DbPool`, `&mut PgConnection`, `&mut SqliteConnection`,
/// or a dereferenced transaction (`&mut *tx`).
pub async fn insert_attachment<'e, E>(executor: E, att: &StorableAttachment) -> Result<()>
where
    E: sqlx::Executor<'e, Database = crate::db::Db>,
{
    #[cfg(feature = "sqlite")]
    let query = sql!(
        "INSERT OR REPLACE INTO attachments (event_id, filename, content_type, data)
         VALUES (?1, ?2, ?3, ?4)"
    );
    #[cfg(not(feature = "sqlite"))]
    let query = sql!("INSERT INTO attachments (event_id, filename, content_type, data)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (event_id, filename) DO UPDATE SET content_type = EXCLUDED.content_type, data = EXCLUDED.data");

    sqlx::query(query)
        .bind(&att.event_id)
        .bind(&att.filename)
        .bind(&att.content_type)
        .bind(&att.data)
        .execute(executor)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ItemType;

    fn txn_with_spans(n: usize) -> StorableEvent {
        let spans: Vec<serde_json::Value> = (0..n)
            .map(|i| {
                serde_json::json!({
                    "span_id": format!("s{i}"),
                    "trace_id": "tr",
                    "parent_span_id": "root",
                    "op": "db.query",
                    "start_timestamp": 1700000000.0 + i as f64 / 1000.0,
                    "timestamp": 1700000000.5 + i as f64 / 1000.0,
                    "status": "ok"
                })
            })
            .collect();
        let payload = serde_json::json!({
            "type": "transaction",
            "transaction": "/api/x",
            "contexts": {"trace": {"trace_id": "tr"}},
            "spans": spans,
        });
        let raw = serde_json::to_vec(&payload).unwrap();
        StorableEvent::new(
            "txn".to_string(),
            ItemType::Transaction,
            raw,
            1,
            "k".to_string(),
        )
    }

    #[test]
    fn embedded_spans_extracted_with_start_ms() {
        let e = txn_with_spans(3);
        let r = &e;
        let refs: Vec<&&StorableEvent> = vec![&r];
        let rows = extract_embedded_spans(&refs);
        assert_eq!(rows.len(), 3);
        for row in &rows {
            assert_eq!(row.trace_id.as_deref(), Some("tr"));
            assert!(row.start_ms.is_some());
        }
    }

    #[test]
    fn embedded_spans_capped_at_1000() {
        let e = txn_with_spans(1500);
        let r = &e;
        let refs: Vec<&&StorableEvent> = vec![&r];
        let rows = extract_embedded_spans(&refs);
        assert_eq!(rows.len(), MAX_EMBEDDED_SPANS);
    }
}
