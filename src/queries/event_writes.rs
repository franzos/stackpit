//! Event write operations shared by both write paths.
//!
//! **Writer thread** (server-side, batched):
//!   `insert_event_row` -> in-memory accumulators -> batched `upsert_issue` / `bulk_upsert_tag_counts` / HLL merge
//!
//! **Sync CLI** (one-shot):
//!   `insert_event_row` -> per-event `upsert_issue_from_event` (tag/HLL upserts included)
//!
//! Both paths share `insert_event_row` and `insert_attachment`. The split is
//! intentional -- the writer batches for throughput, while sync handles events
//! one at a time since it's a CLI import anyway.

use std::collections::HashMap;

use anyhow::Result;
use simple_hll::HyperLogLog;
use sqlx::QueryBuilder;

use crate::db::{sql, DbPool};
use crate::models::{
    ItemType, StorableAttachment, StorableEvent, HLL_REGISTER_COUNT, MAX_TAGS_PER_EVENT,
};

type MetricRow = (
    String,
    String,
    f64,
    Option<String>,
    Option<String>,
    Option<i64>,
);

/// Max events per multi-row INSERT chunk. 19 bind params per event;
/// SQLite's SQLITE_MAX_VARIABLE_NUMBER is 32766, so 32766 / 19 = 1724.
/// We use 1700 for a comfortable margin.
const BULK_CHUNK_SIZE: usize = 1700;

/// Max spans per multi-row INSERT chunk. 13 bind params per span;
/// 32766 / 13 = 2520, use 2300 for margin.
const SPAN_BULK_CHUNK_SIZE: usize = 2300;

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
    let query = sql!("INSERT OR IGNORE INTO events (event_id, item_type, payload, project_id, public_key, timestamp, level, platform, release, environment, server_name, transaction_name, title, sdk_name, sdk_version, fingerprint, monitor_slug, session_status, parent_event_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)");
    #[cfg(not(feature = "sqlite"))]
    let query = sql!("INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, level, platform, release, environment, server_name, transaction_name, title, sdk_name, sdk_version, fingerprint, monitor_slug, session_status, parent_event_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
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
        .execute(executor)
        .await?;

    Ok(result.rows_affected() == 1)
}

struct SpanFields {
    span_id: Option<String>,
    trace_id: Option<String>,
    parent_span_id: Option<String>,
    op: Option<String>,
    description: Option<String>,
    status: Option<String>,
    duration_ms: Option<i64>,
}

/// Map OTEL SpanStatusCode to string. The spec defines only three values:
/// 0 = UNSET, 1 = OK, 2 = ERROR.
fn span_status_from_code(code: u64) -> String {
    match code {
        0 => "ok",
        1 => "ok",
        2 => "internal_error",
        _ => "unknown",
    }
    .to_string()
}

fn extract_span_fields(payload: &[u8]) -> SpanFields {
    let json: Option<serde_json::Value> = zstd::decode_all(payload)
        .ok()
        .or_else(|| Some(payload.to_vec()))
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    match json {
        Some(v) => {
            let duration_ms = v
                .get("timestamp")
                .and_then(|end| v.get("start_timestamp").map(|start| (end, start)))
                .and_then(|(end, start)| {
                    let end_f = end.as_f64()?;
                    let start_f = start.as_f64()?;
                    Some(((end_f - start_f) * 1000.0) as i64)
                });

            SpanFields {
                span_id: v
                    .get("span_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                trace_id: v
                    .get("trace_id")
                    .or_else(|| {
                        v.get("contexts")
                            .and_then(|c| c.get("trace"))
                            .and_then(|t| t.get("trace_id"))
                    })
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                parent_span_id: v
                    .get("parent_span_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                op: v.get("op").and_then(|v| v.as_str()).map(|s| s.to_string()),
                description: v
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                status: v
                    .get("status")
                    .or_else(|| v.get("data").and_then(|d| d.get("status")))
                    .and_then(|v| {
                        v.as_str()
                            .map(|s| s.to_string())
                            .or_else(|| v.as_u64().map(span_status_from_code))
                            .or_else(|| v.as_i64().map(|code| span_status_from_code(code as u64)))
                    }),
                duration_ms,
            }
        }
        None => SpanFields {
            span_id: None,
            trace_id: None,
            parent_span_id: None,
            op: None,
            description: None,
            status: None,
            duration_ms: None,
        },
    }
}

/// Returns (mri, metric_type, value, tags, values_json, bucket_timestamp).
/// bucket_timestamp is Some when the bucket includes its own timestamp.
fn parse_metric_payload(payload: &[u8]) -> Vec<MetricRow> {
    let decoded_bytes = match zstd::decode_all(std::io::Cursor::new(payload)) {
        Ok(bytes) => bytes,
        Err(_) => payload.to_vec(),
    };

    const MAX_METRIC_ENTRIES: usize = 10_000;

    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&decoded_bytes) {
        if let Some(arr) = json.as_array() {
            return arr
                .iter()
                .take(MAX_METRIC_ENTRIES)
                .filter_map(parse_metric_bucket)
                .collect();
        }

        let mri = json
            .get("mri")
            .or_else(|| json.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let metric_type = json
            .get("type")
            .or_else(|| json.get("ty"))
            .and_then(|v| v.as_str())
            .or_else(|| match mri.chars().next() {
                Some('c') => Some("counter"),
                Some('d') => Some("distribution"),
                Some('g') => Some("gauge"),
                Some('s') => Some("set"),
                _ => None,
            })
            .unwrap_or("counter")
            .to_string();

        let (value, values_json) = extract_metric_value(json.get("value"));
        let tags = json.get("tags").map(|t| t.to_string());

        return vec![(mri, metric_type, value, tags, values_json, None)];
    }

    let text = match std::str::from_utf8(&decoded_bytes) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    text.lines()
        .filter(|line| !line.trim().is_empty())
        .take(MAX_METRIC_ENTRIES)
        .map(parse_statsd_line)
        .collect()
}

fn extract_metric_value(v: Option<&serde_json::Value>) -> (f64, Option<String>) {
    match v {
        Some(val) => {
            if let Some(arr) = val.as_array() {
                let values_json = serde_json::to_string(arr).ok();
                let sum: f64 = arr.iter().filter_map(|v| v.as_f64()).sum();
                (sum, values_json)
            } else {
                (val.as_f64().unwrap_or(0.0), None)
            }
        }
        None => (0.0, None),
    }
}

fn parse_metric_bucket(bucket: &serde_json::Value) -> Option<MetricRow> {
    let name = bucket.get("name").and_then(|v| v.as_str())?;
    let unit = bucket
        .get("unit")
        .and_then(|v| v.as_str())
        .unwrap_or("none");
    let metric_type = bucket
        .get("type")
        .or_else(|| bucket.get("ty"))
        .and_then(|v| v.as_str())
        .unwrap_or("counter");

    // Sentry MRI format: {type}:{namespace}/{name}@{unit}
    // If the name already contains a slash (namespace/metric_name), use as-is;
    // otherwise prefix with "custom/" to match Sentry relay normalization.
    let qualified_name = if name.contains('/') {
        name.to_string()
    } else {
        format!("custom/{name}")
    };
    let mri = format!("{metric_type}:{qualified_name}@{unit}");

    let bucket_ts = bucket.get("timestamp").and_then(|v| v.as_i64());
    let (value, values_json) = extract_metric_value(bucket.get("value"));
    let tags = bucket.get("tags").map(|t| t.to_string());

    Some((
        mri,
        metric_type.to_string(),
        value,
        tags,
        values_json,
        bucket_ts,
    ))
}

fn parse_statsd_line(line: &str) -> MetricRow {
    let (name_unit, rest) = match line.split_once(':') {
        Some(parts) => parts,
        None => {
            return (
                line.to_string(),
                "counter".to_string(),
                0.0,
                None,
                None,
                None,
            )
        }
    };

    let (name, unit) = match name_unit.split_once('@') {
        Some((n, u)) => (n, u),
        None => (name_unit, "none"),
    };

    let (value_type, tags_part) = match rest.split_once("|#") {
        Some((vt, tags)) => (vt, Some(tags)),
        None => (rest, None),
    };

    let (value_str, type_str) = match value_type.split_once('|') {
        Some((v, t)) => (v, t),
        None => (value_type, "c"),
    };

    let values: Vec<f64> = value_str
        .split(':')
        .filter_map(|v| v.parse().ok())
        .collect();
    let value: f64 = values.iter().sum();
    let values_json = if values.len() > 1 {
        serde_json::to_string(&values).ok()
    } else {
        None
    };

    let metric_type = match type_str {
        "c" => "counter",
        "g" => "gauge",
        "d" => "distribution",
        "s" => "set",
        "ms" => "distribution",
        other => other,
    }
    .to_string();

    let qualified_name = if name.contains('/') {
        name.to_string()
    } else {
        format!("custom/{name}")
    };
    let mri = format!("{metric_type}:{qualified_name}@{unit}");

    let tags = tags_part.map(|t| {
        let tag_map: serde_json::Map<String, serde_json::Value> = t
            .split(',')
            .filter_map(|pair| {
                let (k, v) = pair.split_once(':')?;
                Some((k.to_string(), serde_json::Value::String(v.to_string())))
            })
            .collect();
        serde_json::Value::Object(tag_map).to_string()
    });

    (mri, metric_type, value, tags, values_json, None)
}

struct LogFields {
    level: Option<String>,
    body: Option<String>,
    trace_id: Option<String>,
    span_id: Option<String>,
    attributes: Option<String>,
}

fn normalize_log_level(level: &str) -> String {
    match level.to_ascii_lowercase().as_str() {
        "warn" | "warning" => "warning".to_string(),
        "err" => "error".to_string(),
        other => other.to_string(),
    }
}

fn extract_log_timestamp(v: &serde_json::Value) -> Option<i64> {
    v.get("timestamp").and_then(|ts| {
        // Try as float (seconds with fractional), then as integer
        if let Some(f) = ts.as_f64() {
            // Distinguish seconds vs milliseconds vs microseconds vs nanoseconds
            if f > 1e18 {
                Some((f / 1e9) as i64)
            }
            // nanoseconds
            else if f > 1e15 {
                Some((f / 1e6) as i64)
            }
            // microseconds
            else if f > 1e12 {
                Some((f / 1e3) as i64)
            }
            // milliseconds
            else {
                Some(f as i64)
            } // seconds
        } else {
            ts.as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|f| f as i64)
        }
    })
}

fn extract_log_fields(v: &serde_json::Value) -> LogFields {
    LogFields {
        level: v
            .get("level")
            .or_else(|| v.get("severity_text"))
            .and_then(|v| v.as_str())
            .map(normalize_log_level),
        body: v
            .get("body")
            .or_else(|| v.get("message"))
            .and_then(|v| {
                v.as_str()
                    .or_else(|| v.get("string_value").and_then(|sv| sv.as_str()))
            })
            .map(String::from),
        trace_id: v.get("trace_id").and_then(|v| v.as_str()).map(String::from),
        span_id: v.get("span_id").and_then(|v| v.as_str()).map(String::from),
        attributes: v.get("attributes").map(|a| a.to_string()),
    }
}

fn parse_log_entries(payload: &[u8]) -> Vec<serde_json::Value> {
    const MAX_LOG_ENTRIES: usize = 10_000;

    let json: Option<serde_json::Value> = zstd::decode_all(std::io::Cursor::new(payload))
        .ok()
        .or_else(|| Some(payload.to_vec()))
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    match json {
        Some(serde_json::Value::Array(mut arr)) => {
            arr.truncate(MAX_LOG_ENTRIES);
            arr
        }
        Some(obj) => {
            // Sentry SDKs wrap structured logs as {"items": [...]}
            if let Some(items) = obj.get("items").and_then(|v| v.as_array()) {
                items.iter().take(MAX_LOG_ENTRIES).cloned().collect()
            } else {
                vec![obj]
            }
        }
        None => Vec::new(),
    }
}

/// Compress a single log entry to its own zstd blob.
fn compress_log_entry(entry: &serde_json::Value) -> Vec<u8> {
    let json_bytes = serde_json::to_vec(entry).unwrap_or_default();
    zstd::encode_all(std::io::Cursor::new(&json_bytes), 3).unwrap_or(json_bytes)
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
    for chunk in events.chunks(BULK_CHUNK_SIZE) {
        #[cfg(feature = "sqlite")]
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "INSERT OR IGNORE INTO events (event_id, item_type, payload, project_id, public_key, timestamp, level, platform, release, environment, server_name, transaction_name, title, sdk_name, sdk_version, fingerprint, monitor_slug, session_status, parent_event_id) "
        );
        #[cfg(not(feature = "sqlite"))]
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, level, platform, release, environment, server_name, transaction_name, title, sdk_name, sdk_version, fingerprint, monitor_slug, session_status, parent_event_id) "
        );

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
        });

        #[cfg(not(feature = "sqlite"))]
        builder.push(" ON CONFLICT (event_id) DO NOTHING");

        builder.build().execute(&mut **tx).await?;
    }

    Ok(())
}

async fn bulk_insert_spans_table(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    spans: &[&&StorableEvent],
) -> Result<()> {
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
    }

    let rows: Vec<SpanRow> = spans
        .iter()
        .map(|event| {
            let fields = extract_span_fields(&event.payload);
            SpanRow {
                span_id: fields.span_id.unwrap_or_else(|| event.event_id.clone()),
                payload: event.payload.clone(),
                project_id: event.project_id as i64,
                public_key: event.public_key.clone(),
                timestamp: event.timestamp,
                release: event.release.clone(),
                environment: event.environment.clone(),
                trace_id: fields.trace_id,
                parent_span_id: fields.parent_span_id,
                op: fields.op,
                description: fields.description,
                status: fields.status,
                duration_ms: fields.duration_ms,
            }
        })
        .collect();

    for chunk in rows.chunks(SPAN_BULK_CHUNK_SIZE) {
        #[cfg(feature = "sqlite")]
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "INSERT OR IGNORE INTO spans (span_id, payload, project_id, public_key, timestamp, release, environment, trace_id, parent_span_id, op, description, status, duration_ms) "
        );
        #[cfg(not(feature = "sqlite"))]
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "INSERT INTO spans (span_id, payload, project_id, public_key, timestamp, release, environment, trace_id, parent_span_id, op, description, status, duration_ms) "
        );

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
        });

        #[cfg(not(feature = "sqlite"))]
        builder.push(" ON CONFLICT (span_id) DO NOTHING");

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

    for chunk in rows.chunks(METRIC_BULK_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "INSERT INTO metrics (project_id, public_key, timestamp, mri, metric_type, value, tags, \"values\") ",
        );

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

    for chunk in rows.chunks(LOG_BULK_CHUNK_SIZE) {
        let mut builder = QueryBuilder::<crate::db::Db>::new(
            "INSERT INTO logs (payload, project_id, public_key, timestamp, release, environment, trace_id, span_id, level, body, attributes) ",
        );

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

    // Tag counts
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

    // User count via HLL — read-modify-write inside the transaction to prevent races
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
