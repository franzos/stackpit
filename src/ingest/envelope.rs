use crate::ingest::auth;
use crate::ingest::auth::SentryAuth;
use crate::ingest::models::{ItemType, StorableAttachment, StorableEvent};
use anyhow::{bail, Result};
use serde_json::Value;

pub struct ParsedEnvelope {
    pub auth: Option<SentryAuth>,
    pub project_id: Option<u64>,
    pub envelope_event_id: Option<String>,
    pub events: Vec<StorableEvent>,
    pub attachments: Vec<StorableAttachment>,
    /// Clock drift correction in seconds (server_received - client_sent_at).
    /// Applied to event timestamps to compensate for client clock skew.
    pub clock_drift_secs: i64,
}

/// Cap on items per envelope; prevents DoS via many small items.
/// Sentry SDKs can send hundreds of spans per envelope, so we allow up to 500.
const MAX_ENVELOPE_ITEMS: usize = 500;

/// Cumulative cap across all accepted item payloads in one envelope. Bounds
/// decompression amplification even if an operator raises `max_body_size`
/// above the per-item large limit. A few large items' worth of headroom.
const MAX_ENVELOPE_TOTAL_BYTES: usize = 4 * crate::ingest::models::MAX_LARGE_ITEM_PAYLOAD_BYTES; // 200MB

/// Parse a Sentry envelope. Wire format: `header\n(item_header\npayload\n)*`.
pub fn parse(body: &[u8], project_id: u64, auth: &SentryAuth) -> Result<ParsedEnvelope> {
    let mut result = ParsedEnvelope {
        auth: None,
        project_id: None,
        envelope_event_id: None,
        events: Vec::new(),
        attachments: Vec::new(),
        clock_drift_secs: 0,
    };

    let first_nl = memchr::memchr(b'\n', body).unwrap_or(body.len());
    let header_bytes = &body[..first_nl];

    if !header_bytes.is_empty() {
        if let Ok(header) = serde_json::from_slice::<Value>(header_bytes) {
            // Some SDKs embed the DSN in the envelope header
            if let Some(dsn) = header.get("dsn").and_then(|v| v.as_str()) {
                if let Some((dsn_auth, dsn_project)) = auth::extract_from_dsn(dsn) {
                    result.auth = Some(dsn_auth);
                    result.project_id = Some(dsn_project);
                }
            }
            // Envelope-level event_id, needed to associate attachments later.
            result.envelope_event_id = header
                .get("event_id")
                .and_then(|v| v.as_str())
                .and_then(crate::ingest::ids::sanitize_id);

            // Clock drift correction: compare sent_at to server receive time.
            // SDKs send ISO 8601 timestamps like "2025-03-07T12:00:00Z".
            if let Some(sent_at_str) = header.get("sent_at").and_then(|v| v.as_str()) {
                if let Ok(sent_at) = chrono::DateTime::parse_from_rfc3339(sent_at_str) {
                    let now = chrono::Utc::now().timestamp();
                    let drift = now - sent_at.timestamp();
                    // Only correct if drift is within a reasonable range (±24h).
                    // Larger drifts likely indicate a bogus sent_at.
                    if drift.abs() <= 86400 {
                        result.clock_drift_secs = drift;
                    }
                }
            }
        }
    }

    // Trust the URL project_id over the DSN one: prevents cross-project
    // injection from a crafted envelope header.
    let effective_project = project_id;
    // Use the request-level auth key, not an envelope header DSN, so events
    // can't be reattributed to another key.
    let effective_key = auth.sentry_key.clone();

    let mut pos = if first_nl < body.len() {
        first_nl + 1
    } else {
        return Ok(result);
    };

    let mut item_count: usize = 0;
    let mut total_payload_bytes: usize = 0;

    while pos < body.len() {
        if item_count >= MAX_ENVELOPE_ITEMS {
            tracing::warn!("envelope exceeded max items limit ({MAX_ENVELOPE_ITEMS}), truncating");
            break;
        }

        let item_nl = memchr::memchr(b'\n', &body[pos..])
            .map(|i| pos + i)
            .unwrap_or(body.len());
        let item_header_bytes = &body[pos..item_nl];

        if item_header_bytes.is_empty() {
            tracing::debug!("skipping empty envelope item header");
            pos = item_nl + 1;
            continue;
        }

        let item_header: Value = match serde_json::from_slice(item_header_bytes) {
            Ok(v) => v,
            Err(_) => {
                // Probably trailing garbage.
                break;
            }
        };

        let item_type_str = item_header
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("event");
        let item_type = item_type_str
            .parse::<ItemType>()
            .unwrap_or(ItemType::Unknown);
        let declared_length = item_header.get("length").and_then(|v| v.as_u64());
        let filename = item_header
            .get("filename")
            .and_then(|v| v.as_str())
            .map(String::from);
        let content_type = item_header
            .get("content_type")
            .or_else(|| item_header.get("content-type"))
            .and_then(|v| v.as_str())
            .map(String::from);

        pos = if item_nl < body.len() {
            item_nl + 1
        } else {
            break;
        };

        // `len_u64` is attacker-declared; checked_add guards against overflow past the bounds check.
        let payload_bytes = if let Some(len_u64) = declared_length {
            let end = usize::try_from(len_u64)
                .ok()
                .and_then(|len| pos.checked_add(len))
                .filter(|&e| e <= body.len());
            if let Some(end) = end {
                let slice = &body[pos..end];
                pos = end;
                // Trailing newline after length-prefixed payload
                if pos < body.len() && body[pos] == b'\n' {
                    pos += 1;
                }
                slice
            } else {
                // A declared length past the body desyncs the item stream, so we
                // can't locate the next item; skip the rest rather than accept a
                // truncated payload as a real item.
                tracing::warn!(
                    "envelope item declared length {len_u64} exceeds remaining body ({} bytes), skipping",
                    body.len() - pos
                );
                pos = body.len();
                continue;
            }
        } else {
            // No declared length: read until the next newline.
            let end = memchr::memchr(b'\n', &body[pos..])
                .map(|i| pos + i)
                .unwrap_or(body.len());
            let slice = &body[pos..end];
            pos = if end < body.len() { end + 1 } else { end };
            slice
        };

        if payload_bytes.is_empty() {
            continue;
        }

        let size_limit = item_type.max_payload_bytes();
        if payload_bytes.len() > size_limit {
            tracing::warn!(
                "envelope item exceeds max size ({} > {size_limit}), skipping",
                payload_bytes.len()
            );
            continue;
        }

        total_payload_bytes = total_payload_bytes.saturating_add(payload_bytes.len());
        if total_payload_bytes > MAX_ENVELOPE_TOTAL_BYTES {
            tracing::warn!(
                "envelope cumulative payload exceeds cap ({total_payload_bytes} > {MAX_ENVELOPE_TOTAL_BYTES}), truncating"
            );
            break;
        }

        item_count += 1;

        if item_type == ItemType::Attachment {
            result.attachments.push(StorableAttachment {
                event_id: String::new(), // caller fills this in
                filename: filename.unwrap_or_else(|| "unknown".to_string()),
                content_type,
                data: payload_bytes.to_vec(),
            });
            continue;
        }

        let mut event = StorableEvent::new(
            String::new(), // placeholder; extract_fields sets it
            item_type,
            payload_bytes.to_vec(),
            effective_project,
            effective_key.clone(),
        );

        let parsed_event_id = extract_fields(
            payload_bytes,
            &item_type,
            &mut event,
            result.clock_drift_secs,
        );

        // UserReport's event_id refers to the parent event; give it its own UUID.
        if item_type == ItemType::UserReport {
            event.parent_event_id = parsed_event_id;
            event.event_id = uuid::Uuid::new_v4().to_string();
        } else {
            event.event_id = parsed_event_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        }

        result.events.push(event);
    }

    Ok(result)
}

/// Test-only shim so other modules can exercise field extraction without
/// reconstructing a full envelope.
#[cfg(test)]
pub(crate) fn extract_fields_for_test(
    payload: &[u8],
    item_type: &ItemType,
    event: &mut StorableEvent,
) {
    extract_fields(payload, item_type, event, 0);
}

/// Pull known fields out of the JSON payload into a StorableEvent.
/// Returns the event_id if one was present.
///
/// `drift` is added to client-supplied timestamps only; anything that falls back
/// to `event.timestamp` (itself server-authoritative unless the payload carried a
/// timestamp) inherits that value's correction, so nothing is corrected twice.
fn extract_fields(
    payload: &[u8],
    item_type: &ItemType,
    event: &mut StorableEvent,
    drift: i64,
) -> Option<String> {
    match item_type {
        // Not JSON: a header line followed by compressed rrweb / video bytes.
        // The full parse always failed on these, so the generated id is the
        // same outcome without the attempt.
        ItemType::ReplayRecording | ItemType::ReplayVideo => None,
        // Up to 50 MiB, mostly `profile.samples`; only the head is wanted.
        ItemType::Profile | ItemType::ProfileChunk => {
            extract_fields_head(payload, item_type, event, drift)
        }
        _ => extract_fields_full(payload, item_type, event, drift),
    }
}

/// Magnitude distinguishes s / ms / us / ns, normalized to seconds: seconds up
/// to ~1e11 (year ~5138), milliseconds to 1e14, microseconds to 1e17,
/// nanoseconds above.
fn timestamp_from_value(v: &Value) -> Option<i64> {
    v.as_f64()
        .filter(|f| f.is_finite())
        .map(|f| {
            if f > 1e17 {
                (f / 1e9).round() as i64
            } else if f > 1e14 {
                (f / 1e6).round() as i64
            } else if f > 1e11 {
                (f / 1e3).round() as i64
            } else {
                f.round() as i64
            }
        })
        .or_else(|| {
            v.as_i64().map(|i| {
                if i > 100_000_000_000_000_000 {
                    i / 1_000_000_000
                } else if i > 100_000_000_000_000 {
                    i / 1_000_000
                } else if i > 100_000_000_000 {
                    i / 1_000
                } else {
                    i
                }
            })
        })
}

/// First of `user.id` (string or integer), `email`, `username`, `ip_address`.
fn user_identifier_from(
    id: Option<&Value>,
    email: Option<&Value>,
    username: Option<&Value>,
    ip_address: Option<&Value>,
) -> Option<String> {
    id.and_then(|v| {
        v.as_str()
            .map(String::from)
            .or_else(|| v.as_u64().map(|n| n.to_string()))
    })
    .or_else(|| email.and_then(|v| v.as_str()).map(String::from))
    .or_else(|| username.and_then(|v| v.as_str()).map(String::from))
    .or_else(|| ip_address.and_then(|v| v.as_str()).map(String::from))
}

fn str_field(v: Option<&Value>) -> Option<String> {
    v.and_then(|v| v.as_str()).map(String::from)
}

/// The few top-level keys a profile item contributes to its row. serde skips
/// every other key without building it, which is the whole point. Every field
/// is a `Value` so a wrong-shaped one degrades exactly as `.get()` chains do on
/// the full document instead of failing the whole parse.
#[derive(Default, serde::Deserialize)]
struct EventHead {
    event_id: Option<Value>,
    timestamp: Option<Value>,
    level: Option<Value>,
    severity_text: Option<Value>,
    platform: Option<Value>,
    release: Option<Value>,
    environment: Option<Value>,
    server_name: Option<Value>,
    transaction: Option<Value>,
    monitor_slug: Option<Value>,
    contexts: Option<Value>,
    sdk: Option<Value>,
    user: Option<Value>,
    tags: Option<Value>,
    exception: Option<Value>,
    message: Option<Value>,
    logentry: Option<Value>,
}

/// Same assignments as `extract_fields_full`, fed from the typed head. Profile
/// items never fingerprint, so only the title helper needs a JSON view, built
/// from the handful of keys it reads.
fn extract_fields_head(
    payload: &[u8],
    item_type: &ItemType,
    event: &mut StorableEvent,
    drift: i64,
) -> Option<String> {
    // serde would fill the struct positionally from a top-level array, so a
    // non-object document takes the full path, which extracts nothing from it.
    let starts_object = payload
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| *b == b'{');
    if !starts_object {
        return extract_fields_full(payload, item_type, event, drift);
    }
    let head: EventHead = match serde_json::from_slice(payload) {
        Ok(h) => h,
        Err(_) => return None,
    };

    let event_id = head
        .event_id
        .as_ref()
        .and_then(|v| v.as_str())
        .and_then(crate::ingest::ids::sanitize_id);

    if let Some(ts) = head.timestamp.as_ref().and_then(timestamp_from_value) {
        event.timestamp = ts + drift;
    }

    event.level = head
        .level
        .as_ref()
        .or(head.severity_text.as_ref())
        .and_then(|v| v.as_str())
        .map(|s| {
            s.parse::<crate::ingest::models::Level>()
                .unwrap_or(crate::ingest::models::Level::Unknown)
        });
    event.platform = str_field(head.platform.as_ref());
    event.release = str_field(head.release.as_ref());
    event.environment = str_field(head.environment.as_ref());
    event.server_name = str_field(head.server_name.as_ref());
    event.transaction_name = str_field(head.transaction.as_ref());
    event.monitor_slug = str_field(head.monitor_slug.as_ref());

    if event.trace_id.is_none() {
        event.trace_id = head
            .contexts
            .as_ref()
            .and_then(|c| c.get("trace"))
            .and_then(|t| t.get("trace_id"))
            .and_then(|v| v.as_str())
            .map(String::from);
    }

    if let Some(sdk) = &head.sdk {
        event.sdk_name = str_field(sdk.get("name"));
        event.sdk_version = str_field(sdk.get("version"));
    }

    event.user_identifier = head.user.as_ref().and_then(|u| {
        user_identifier_from(
            u.get("id"),
            u.get("email"),
            u.get("username"),
            u.get("ip_address"),
        )
    });

    event.tags = extract_tags(head.tags.as_ref());

    event.fingerprint = None;
    let mut title_src = serde_json::Map::new();
    for (key, value) in [
        ("exception", head.exception),
        ("message", head.message),
        ("logentry", head.logentry),
        ("transaction", head.transaction),
    ] {
        if let Some(v) = value {
            title_src.insert(key.to_string(), v);
        }
    }
    event.title = crate::ingest::enrich::extract_title_from(
        &Value::Object(title_src),
        item_type,
        event.monitor_slug.as_deref(),
    );

    event_id
}

fn extract_fields_full(
    payload: &[u8],
    item_type: &ItemType,
    event: &mut StorableEvent,
    drift: i64,
) -> Option<String> {
    let json: Value = match serde_json::from_slice(payload) {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Log items may arrive as a JSON array or {"items": [...]} batch. Extract
    // the per-entry data here while the parsed JSON is in hand; return None so
    // the event gets a generated UUID.
    if *item_type == ItemType::Log
        && (json.is_array() || json.get("items").and_then(|v| v.as_array()).is_some())
    {
        let mut entries: Vec<_> = crate::ingest::parse_log::log_entries_from_value(json)
            .iter()
            .map(crate::ingest::parse_log::parse_log_entry)
            .collect();
        shift_log_entries(&mut entries, drift);
        event.log_entries = Some(entries);
        return None;
    }

    let event_id = json
        .get("event_id")
        .and_then(|v| v.as_str())
        .and_then(crate::ingest::ids::sanitize_id);

    if let Some(ts) = json.get("timestamp").and_then(timestamp_from_value) {
        event.timestamp = ts + drift;
    }

    event.level = json
        .get("level")
        .or_else(|| json.get("severity_text"))
        .and_then(|v| v.as_str())
        .map(|s| {
            s.parse::<crate::ingest::models::Level>()
                .unwrap_or(crate::ingest::models::Level::Unknown)
        });
    event.platform = json
        .get("platform")
        .and_then(|v| v.as_str())
        .map(String::from);
    event.release = json
        .get("release")
        .and_then(|v| v.as_str())
        .map(String::from);
    event.environment = json
        .get("environment")
        .and_then(|v| v.as_str())
        .map(String::from);
    event.server_name = json
        .get("server_name")
        .and_then(|v| v.as_str())
        .map(String::from);
    event.transaction_name = json
        .get("transaction")
        .and_then(|v| v.as_str())
        .map(String::from);
    event.monitor_slug = json
        .get("monitor_slug")
        .and_then(|v| v.as_str())
        .map(String::from);

    if *item_type == ItemType::Session {
        event.session_status = json
            .get("status")
            .and_then(|v| v.as_str())
            .map(String::from);
        extract_session_bucket(&json, event, drift);
    } else if *item_type == ItemType::Sessions {
        extract_session_aggregates(&json, event, drift);
    } else if *item_type == ItemType::Transaction {
        extract_transaction_perf(&json, event);
        let mut spans = crate::ingest::parse_span::extract_embedded_spans_from_value(&json);
        for span in &mut spans {
            if let Some(ts) = span.timestamp.as_mut() {
                *ts += drift;
            }
            shift_span_start(&mut span.fields, drift);
        }
        event.embedded_spans = Some(spans);
    } else if *item_type == ItemType::Span {
        let mut fields = crate::ingest::parse_span::extract_span_fields_from_value(&json);
        shift_span_start(&mut fields, drift);
        event.span_fields = Some(fields);
    }

    // Error and default events also carry a trace context; capture trace_id so
    // they correlate to the trace waterfall.
    if event.trace_id.is_none() {
        event.trace_id = json
            .get("contexts")
            .and_then(|c| c.get("trace"))
            .and_then(|t| t.get("trace_id"))
            .and_then(|v| v.as_str())
            .map(String::from);
    }

    if let Some(sdk) = json.get("sdk") {
        event.sdk_name = sdk.get("name").and_then(|v| v.as_str()).map(String::from);
        event.sdk_version = sdk
            .get("version")
            .and_then(|v| v.as_str())
            .map(String::from);
    }

    event.user_identifier = json.get("user").and_then(|u| {
        user_identifier_from(
            u.get("id"),
            u.get("email"),
            u.get("username"),
            u.get("ip_address"),
        )
    });

    event.tags = extract_tags(json.get("tags"));

    // Compute fingerprint and title from the already-parsed JSON so
    // enrich_event won't need to re-parse the payload
    event.fingerprint = crate::ingest::fingerprint::compute_fingerprint_from_value(
        event.project_id,
        item_type,
        &json,
    );
    event.title =
        crate::ingest::enrich::extract_title_from(&json, item_type, event.monitor_slug.as_deref());

    // Single-object log item (batches returned early above).
    if *item_type == ItemType::Log {
        let mut entries = vec![crate::ingest::parse_log::parse_log_entry(&json)];
        shift_log_entries(&mut entries, drift);
        event.log_entries = Some(entries);
    }

    event_id
}

fn shift_log_entries(entries: &mut [crate::ingest::parse_log::ParsedLogEntry], drift: i64) {
    if drift == 0 {
        return;
    }
    for entry in entries {
        if let Some(ts) = entry.timestamp.as_mut() {
            *ts += drift;
        }
    }
}

fn shift_span_start(fields: &mut crate::ingest::parse_span::SpanFields, drift: i64) {
    if let Some(start_ms) = fields.start_ms.as_mut() {
        *start_ms += drift * 1000;
    }
}

/// Pull trace_id, duration, and trace status off a transaction payload.
/// Duration prefers `measurements.duration.value` (already ms); otherwise it's
/// derived from the raw `start_timestamp`/`timestamp` floats (seconds).
fn extract_transaction_perf(json: &Value, event: &mut StorableEvent) {
    let trace = json.get("contexts").and_then(|c| c.get("trace"));
    event.trace_id = trace
        .and_then(|t| t.get("trace_id"))
        .and_then(|v| v.as_str())
        .map(String::from);
    event.trace_status = trace
        .and_then(|t| t.get("status"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let measured = json
        .get("measurements")
        .and_then(|m| m.get("duration"))
        .and_then(|d| d.get("value"))
        .and_then(serde_json::Value::as_f64)
        .filter(|f| f.is_finite());

    event.duration_ms = match measured {
        Some(ms) => Some(ms.round() as i64),
        None => {
            let end = json.get("timestamp").and_then(serde_json::Value::as_f64);
            let start = json
                .get("start_timestamp")
                .and_then(serde_json::Value::as_f64);
            match (end, start) {
                (Some(e), Some(s)) if e.is_finite() && s.is_finite() => {
                    Some(((e - s) * 1000.0).round() as i64)
                }
                _ => None,
            }
        }
    };
}

/// Read release/environment from a session item's `attrs`, defaulting to ''.
fn session_attrs(json: &Value) -> (String, String) {
    let attrs = json.get("attrs");
    let release = attrs
        .and_then(|a| a.get("release"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let environment = attrs
        .and_then(|a| a.get("environment"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    (release, environment)
}

/// Parse a single `session` item into one SessionBucket.
fn extract_session_bucket(json: &Value, event: &mut StorableEvent, drift: i64) {
    let (release, environment) = session_attrs(json);
    let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("ok");
    let errors = json.get("errors").and_then(|v| v.as_u64()).unwrap_or(0);
    let init = json.get("init").and_then(|v| v.as_bool()).unwrap_or(false);
    let did = json.get("did").and_then(|v| v.as_str()).map(String::from);

    let (mut crashed, mut errored, mut abnormal) = (0u64, 0u64, 0u64);
    if status == "crashed" {
        crashed = 1;
    } else if status == "abnormal" {
        abnormal = 1;
    } else if errors > 0 {
        errored = 1;
    }
    // total counts the session only on its init update, avoiding double-counting
    // per-update heartbeats while still letting the terminal crash/abnormal
    // update contribute to the failure counters.
    let total = u64::from(init);

    // The session's own start time is an rfc3339 string the generic timestamp
    // path doesn't parse, so derive it here; otherwise every session buckets to
    // the ingestion time and the daily trend collapses to one day.
    let started_ts = json
        .get("started")
        .or_else(|| json.get("timestamp"))
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map_or(event.timestamp, |dt| dt.timestamp() + drift);

    event
        .session_buckets
        .push(crate::ingest::models::SessionBucket {
            release,
            environment,
            started_ts,
            total,
            crashed,
            errored,
            abnormal,
            did,
            is_aggregate: false,
        });
}

/// Parse a `sessions` aggregate item into one SessionBucket per `aggregates[]` entry.
fn extract_session_aggregates(json: &Value, event: &mut StorableEvent, drift: i64) {
    let (release, environment) = session_attrs(json);
    let Some(aggregates) = json.get("aggregates").and_then(|v| v.as_array()) else {
        return;
    };

    for agg in aggregates {
        let exited = agg.get("exited").and_then(|v| v.as_u64()).unwrap_or(0);
        let errored = agg.get("errored").and_then(|v| v.as_u64()).unwrap_or(0);
        let crashed = agg.get("crashed").and_then(|v| v.as_u64()).unwrap_or(0);
        let abnormal = agg.get("abnormal").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = exited + errored + crashed + abnormal;

        let started_ts = agg
            .get("started")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map_or(event.timestamp, |dt| dt.timestamp() + drift);

        event
            .session_buckets
            .push(crate::ingest::models::SessionBucket {
                release: release.clone(),
                environment: environment.clone(),
                started_ts,
                total,
                crashed,
                errored,
                abnormal,
                did: None,
                is_aggregate: true,
            });
    }
}

/// Tags from Sentry arrive as either `[["key", "value"], ...]` or
/// `{"key": "value", ...}`; both shapes occur in the wild.
fn extract_tags(tags: Option<&Value>) -> Vec<(String, String)> {
    let Some(tags) = tags else {
        return Vec::new();
    };

    let mut result = Vec::new();
    match tags {
        Value::Array(arr) => {
            for pair in arr {
                if let Some(inner) = pair.as_array() {
                    if inner.len() == 2 {
                        let key = inner[0].as_str().unwrap_or("").to_string();
                        let value = inner[1].as_str().unwrap_or("").to_string();
                        if !key.is_empty() {
                            result.push((key, value));
                        }
                    }
                }
            }
        }
        Value::Object(map) => {
            for (key, val) in map {
                let value = val
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| val.to_string());
                result.push((key.clone(), value));
            }
        }
        _ => {}
    }

    result
}

/// The legacy `/store/` endpoint sends a plain JSON body, no envelope framing.
pub fn parse_store_body(body: &[u8], project_id: u64, auth: &SentryAuth) -> Result<StorableEvent> {
    if body.is_empty() {
        bail!("empty body");
    }

    let mut event = StorableEvent::new(
        String::new(),
        ItemType::Event,
        body.to_vec(),
        project_id,
        auth.sentry_key.clone(),
    );

    let event_id = extract_fields(body, &ItemType::Event, &mut event, 0);
    event.event_id = event_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    Ok(event)
}

/// Wrap a CSP report into a Sentry event, normalizing the browser format.
pub fn parse_security_body(
    body: &[u8],
    project_id: u64,
    auth: &SentryAuth,
) -> Result<StorableEvent> {
    if body.is_empty() {
        bail!("empty body");
    }

    let raw: Value = serde_json::from_slice(body)
        .map_err(|e| anyhow::anyhow!("invalid JSON in security report: {e}"))?;

    // CSP reports arrive as {"csp-report": {...}}; grouped by directive.
    let csp_report = raw.get("csp-report").unwrap_or(&raw);
    let directive = csp_report
        .get("violated-directive")
        .or_else(|| csp_report.get("effective-directive"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let title = format!("CSP: {directive}");

    // Message uses only the directive so different blocked URIs with the
    // same directive group together.
    let wrapper = serde_json::json!({
        "event_id": uuid::Uuid::new_v4().to_string(),
        "level": "warning",
        "logger": "csp",
        "platform": "other",
        "message": title,
        "csp": raw,
    });

    let payload = serde_json::to_vec(&wrapper)?;

    let mut event = StorableEvent::new(
        wrapper["event_id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        ItemType::Event,
        payload,
        project_id,
        auth.sentry_key.clone(),
    );
    event.level = Some(crate::ingest::models::Level::Warning);
    event.platform = Some("other".to_string());
    Ok(event)
}

/// Minidump uploads: little to extract, but stored as events.
pub fn parse_minidump(event_id: &str, project_id: u64, public_key: &str) -> Result<StorableEvent> {
    let wrapper = serde_json::json!({
        "event_id": event_id,
        "level": "error",
        "platform": "native",
    });
    let payload = serde_json::to_vec(&wrapper)?;

    let mut event = StorableEvent::new(
        event_id.to_string(),
        ItemType::Event,
        payload,
        project_id,
        public_key.to_string(),
    );
    event.level = Some(crate::ingest::models::Level::Error);
    event.platform = Some("native".to_string());
    event.title = Some("Minidump".to_string());
    event.fingerprint = crate::ingest::fingerprint::compute_fingerprint_from_value(
        project_id,
        &ItemType::Event,
        &wrapper,
    );
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_auth() -> SentryAuth {
        SentryAuth {
            sentry_key: "testkey".to_string(),
        }
    }

    fn profile_payload(samples: usize) -> Vec<u8> {
        let samples: Vec<Value> = (0..samples)
            .map(|i| {
                serde_json::json!({
                    "stack_id": i % 7,
                    "thread_id": "1",
                    "elapsed_since_start_ns": i * 1000,
                })
            })
            .collect();
        serde_json::to_vec(&serde_json::json!({
            "event_id": "abcdef0123456789abcdef0123456789",
            "timestamp": 1_700_000_000.5,
            "platform": "python",
            "release": "app@1.2.3",
            "environment": "prod",
            "transaction": "/api/orders",
            "sdk": {
                "name": "sentry.python",
                "version": "2.0.0",
                "packages": [{"name": "pypi:sentry-sdk", "version": "2.0.0"}],
            },
            "user": {"id": 42, "email": "u@example.com"},
            "tags": {"region": "eu", "tier": "gold"},
            "contexts": {
                "trace": {"trace_id": "0af7651916cd43dd8448eb211c80319c", "span_id": "b7ad6b7169203331"},
                "device": {"arch": "arm64"},
            },
            "profile": {
                "samples": samples,
                "stacks": [[0, 1, 2]],
                "frames": [{"function": "main"}],
            },
        }))
        .unwrap()
    }

    // The typed head must store exactly what the full parse stored. The
    // literal values are the fixture captured from the full parse; the second
    // half pins the head path to it field by field.
    #[test]
    fn profile_head_matches_full_parse_fixture() {
        let raw = profile_payload(3000);
        let mut full = StorableEvent::test_default("x");
        let full_id = extract_fields_full(&raw, &ItemType::Profile, &mut full, 7);

        assert_eq!(full_id.as_deref(), Some("abcdef0123456789abcdef0123456789"));
        assert_eq!(full.timestamp, 1_700_000_001 + 7);
        assert_eq!(full.level, None);
        assert_eq!(full.platform.as_deref(), Some("python"));
        assert_eq!(full.release.as_deref(), Some("app@1.2.3"));
        assert_eq!(full.environment.as_deref(), Some("prod"));
        assert_eq!(full.server_name, None);
        assert_eq!(full.transaction_name.as_deref(), Some("/api/orders"));
        assert_eq!(full.monitor_slug, None);
        assert_eq!(
            full.trace_id.as_deref(),
            Some("0af7651916cd43dd8448eb211c80319c")
        );
        assert_eq!(full.sdk_name.as_deref(), Some("sentry.python"));
        assert_eq!(full.sdk_version.as_deref(), Some("2.0.0"));
        assert_eq!(full.user_identifier.as_deref(), Some("42"));
        assert_eq!(
            full.tags,
            vec![
                ("region".to_string(), "eu".to_string()),
                ("tier".to_string(), "gold".to_string()),
            ]
        );
        assert_eq!(full.fingerprint, None);
        assert_eq!(full.title.as_deref(), Some("/api/orders"));

        let mut head = StorableEvent::test_default("x");
        let head_id = extract_fields_head(&raw, &ItemType::Profile, &mut head, 7);
        assert_eq!(head_id, full_id);
        assert_eq!(head.timestamp, full.timestamp);
        assert_eq!(head.level, full.level);
        assert_eq!(head.platform, full.platform);
        assert_eq!(head.release, full.release);
        assert_eq!(head.environment, full.environment);
        assert_eq!(head.server_name, full.server_name);
        assert_eq!(head.transaction_name, full.transaction_name);
        assert_eq!(head.monitor_slug, full.monitor_slug);
        assert_eq!(head.trace_id, full.trace_id);
        assert_eq!(head.sdk_name, full.sdk_name);
        assert_eq!(head.sdk_version, full.sdk_version);
        assert_eq!(head.user_identifier, full.user_identifier);
        assert_eq!(head.tags, full.tags);
        assert_eq!(head.fingerprint, full.fingerprint);
        assert_eq!(head.title, full.title);
    }

    // Shape drift in one nested object must cost only that object's fields,
    // and a non-object document must yield nothing, exactly like the full parse.
    #[test]
    fn profile_head_degrades_like_the_full_parse_on_odd_shapes() {
        let odd = serde_json::to_vec(&serde_json::json!({
            "event_id": "abcdef0123456789abcdef0123456789",
            "platform": "python",
            "user": "anonymized",
            "sdk": ["sentry.python"],
            "contexts": {"trace": "not-an-object"},
            "tags": 7,
        }))
        .unwrap();
        for payload in [
            odd.as_slice(),
            br#"["abcdef0123456789abcdef0123456789", 1700000000, "error"]"#.as_slice(),
            b"42".as_slice(),
        ] {
            let mut full = StorableEvent::test_default("x");
            let full_id = extract_fields_full(payload, &ItemType::Profile, &mut full, 0);
            let mut head = StorableEvent::test_default("x");
            let head_id = extract_fields_head(payload, &ItemType::Profile, &mut head, 0);
            assert_eq!(head_id, full_id);
            assert_eq!(head.platform, full.platform);
            assert_eq!(head.user_identifier, full.user_identifier);
            assert_eq!(head.sdk_name, full.sdk_name);
            assert_eq!(head.trace_id, full.trace_id);
            assert_eq!(head.tags, full.tags);
            assert_eq!(head.title, full.title);
        }
    }

    // Recording blobs are not JSON: the full parse always failed and the item
    // got a generated id with nothing extracted. Skipping the parse must land
    // in exactly that state.
    #[test]
    fn replay_recording_skips_the_parse_and_gets_a_generated_id() {
        let payload = b"{\"segment_id\":1}\n\x1f\x8b\x08\x00binary-rrweb-bytes";
        let mut before = StorableEvent::new(
            String::new(),
            ItemType::ReplayRecording,
            payload.to_vec(),
            1,
            "testkey".to_string(),
        );
        assert_eq!(
            extract_fields_full(payload, &ItemType::ReplayRecording, &mut before, 0),
            None
        );
        assert_eq!(before.title, None);
        assert_eq!(before.platform, None);

        let body = format!(
            "{{}}\n{{\"type\":\"replay_recording\",\"length\":{}}}\n",
            payload.len()
        );
        let mut body = body.into_bytes();
        body.extend_from_slice(payload);
        body.push(b'\n');

        let result = parse(&body, 1, &test_auth()).unwrap();
        assert_eq!(result.events.len(), 1);
        let ev = &result.events[0];
        assert_eq!(ev.item_type, ItemType::ReplayRecording);
        assert!(uuid::Uuid::parse_str(&ev.event_id).is_ok());
        assert_eq!(ev.platform, before.platform);
        assert_eq!(ev.title, before.title);
        assert_eq!(ev.tags, before.tags);
        assert_eq!(ev.user_identifier, before.user_identifier);
        assert_eq!(ev.fingerprint, None);
    }

    // --- parse ---

    #[test]
    fn parse_single_event_newline_delimited() {
        let event_json = r#"{"event_id":"aaa","message":"hello","timestamp":1000}"#;
        let body = format!("{{}}\n{{\"type\":\"event\"}}\n{event_json}\n");

        let mut result = parse(body.as_bytes(), 1, &test_auth()).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].event_id, "aaa");
        assert_eq!(result.events[0].item_type, ItemType::Event);
        assert_eq!(result.events[0].project_id, 1);
        // Title comes from enrichment, not from parse
        crate::ingest::enrich::enrich_event(&mut result.events[0]);
        assert_eq!(result.events[0].title.as_deref(), Some("hello"));
    }

    /// An `event_id` that would split mid-char when a template truncates it is
    /// dropped at ingest; the event still stores, under a generated id.
    #[test]
    fn parse_rejects_non_ascii_event_id() {
        let event_json = r#"{"event_id":"aaaaaaaaaaaé","message":"hello"}"#;
        let body = format!("{{}}\n{{\"type\":\"event\"}}\n{event_json}\n");

        let result = parse(body.as_bytes(), 1, &test_auth()).unwrap();
        assert_eq!(result.events.len(), 1);
        let id = &result.events[0].event_id;
        assert_ne!(id, "aaaaaaaaaaaé");
        assert!(
            uuid::Uuid::parse_str(id).is_ok(),
            "expected a generated uuid"
        );
    }

    #[test]
    fn parse_rejects_overlong_event_id() {
        let long = "a".repeat(200);
        let event_json = format!(r#"{{"event_id":"{long}","message":"hello"}}"#);
        let body = format!("{{}}\n{{\"type\":\"event\"}}\n{event_json}\n");

        let result = parse(body.as_bytes(), 1, &test_auth()).unwrap();
        assert_eq!(result.events.len(), 1);
        assert!(uuid::Uuid::parse_str(&result.events[0].event_id).is_ok());
    }

    #[test]
    fn parse_length_prefixed_item() {
        let event_json = r#"{"event_id":"bbb","message":"hi"}"#;
        let len = event_json.len();
        let body = format!("{{}}\n{{\"type\":\"event\",\"length\":{len}}}\n{event_json}\n");

        let result = parse(body.as_bytes(), 1, &test_auth()).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].event_id, "bbb");
    }

    #[test]
    fn parse_dsn_from_envelope_header() {
        let body =
            b"{\"dsn\":\"https://envkey@host/99\"}\n{\"type\":\"event\"}\n{\"event_id\":\"c\"}\n";

        let result = parse(body, 1, &test_auth()).unwrap();
        assert_eq!(result.project_id, Some(99));
        assert_eq!(result.auth.as_ref().unwrap().sentry_key, "envkey");
        // URL project_id wins over DSN project_id (security measure).
        assert_eq!(result.events[0].project_id, 1);
        // Request-level auth key always wins over envelope DSN key
        assert_eq!(result.events[0].public_key, "testkey");
    }

    #[test]
    fn parse_envelope_event_id_from_header() {
        let body =
            b"{\"event_id\":\"env-level-id\"}\n{\"type\":\"event\"}\n{\"event_id\":\"e1\"}\n";
        let result = parse(body, 1, &test_auth()).unwrap();
        assert_eq!(result.envelope_event_id.as_deref(), Some("env-level-id"));
    }

    #[test]
    fn parse_envelope_event_id_none_when_absent() {
        let body = b"{}\n{\"type\":\"event\"}\n{\"event_id\":\"e1\"}\n";
        let result = parse(body, 1, &test_auth()).unwrap();
        assert!(result.envelope_event_id.is_none());
    }

    #[test]
    fn parse_multiple_items() {
        let body = b"{}\n{\"type\":\"event\"}\n{\"event_id\":\"e1\"}\n{\"type\":\"transaction\"}\n{\"event_id\":\"e2\"}\n";

        let result = parse(body, 5, &test_auth()).unwrap();
        assert_eq!(result.events.len(), 2);
        assert_eq!(result.events[0].item_type, ItemType::Event);
        assert_eq!(result.events[1].item_type, ItemType::Transaction);
    }

    #[test]
    fn parse_attachment_item() {
        let data = b"binary-data-here";
        let len = data.len();
        let header = format!(
            "{{}}\n{{\"type\":\"attachment\",\"filename\":\"file.txt\",\"length\":{len}}}\n"
        );
        let mut body = header.into_bytes();
        body.extend_from_slice(data);
        body.push(b'\n');

        let result = parse(&body, 1, &test_auth()).unwrap();
        assert_eq!(result.events.len(), 0);
        assert_eq!(result.attachments.len(), 1);
        assert_eq!(result.attachments[0].filename, "file.txt");
        assert_eq!(result.attachments[0].data, data);
    }

    #[test]
    fn parse_empty_body_items_skipped() {
        // Two newlines in a row: empty payload, should be skipped.
        let body = b"{}\n{\"type\":\"event\"}\n\n";
        let result = parse(body, 1, &test_auth()).unwrap();
        assert_eq!(result.events.len(), 0);
    }

    #[test]
    fn parse_header_only() {
        let body = b"{}";
        let result = parse(body, 1, &test_auth()).unwrap();
        assert!(result.events.is_empty());
    }

    // --- parse_store_body ---

    #[test]
    fn parse_store_body_valid_json() {
        let body = br#"{"event_id":"store1","level":"error","message":"boom","timestamp":5000}"#;
        let event = parse_store_body(body, 7, &test_auth()).unwrap();
        assert_eq!(event.event_id, "store1");
        assert_eq!(event.item_type, ItemType::Event);
        assert_eq!(event.project_id, 7);
        assert_eq!(event.level, Some(crate::ingest::models::Level::Error));
        assert_eq!(event.timestamp, 5000);
        // Title is computed in extract_fields (same JSON parse).
        assert_eq!(event.title.as_deref(), Some("boom"));
    }

    #[test]
    fn parse_store_body_empty() {
        let result = parse_store_body(b"", 1, &test_auth());
        assert!(result.is_err());
    }

    // --- parse_security_body ---

    #[test]
    fn parse_security_body_wraps_csp() {
        let body = br#"{"csp-report":{"document-uri":"https://example.com","violated-directive":"script-src","blocked-uri":"https://evil.com"}}"#;
        let mut event = parse_security_body(body, 3, &test_auth()).unwrap();
        assert_eq!(event.item_type, ItemType::Event);
        assert_eq!(event.project_id, 3);
        assert_eq!(event.level, Some(crate::ingest::models::Level::Warning));
        assert_eq!(event.platform.as_deref(), Some("other"));

        // Payload is still raw JSON before finalize
        let json: Value = serde_json::from_slice(&event.payload).unwrap();
        assert!(json.get("csp").is_some());
        assert_eq!(
            json.get("message").and_then(|v| v.as_str()),
            Some("CSP: script-src")
        );

        // After enrich: title extracted, payload stays raw JSON
        // (compression happens in the writer task).
        crate::ingest::enrich::enrich_event(&mut event);
        assert_eq!(event.title.as_deref(), Some("CSP: script-src"));
        let json2: Value = serde_json::from_slice(&event.payload).unwrap();
        assert!(json2.get("csp").is_some());
    }

    #[test]
    fn csp_reports_with_same_directive_group_together() {
        let body1 =
            br#"{"csp-report":{"violated-directive":"script-src","blocked-uri":"https://a.com"}}"#;
        let body2 =
            br#"{"csp-report":{"violated-directive":"script-src","blocked-uri":"https://b.com"}}"#;
        let mut event1 = parse_security_body(body1, 3, &test_auth()).unwrap();
        let mut event2 = parse_security_body(body2, 3, &test_auth()).unwrap();
        crate::ingest::enrich::enrich_event(&mut event1);
        crate::ingest::enrich::enrich_event(&mut event2);
        // Same directive, different blocked URI: should group together.
        assert_eq!(event1.fingerprint, event2.fingerprint);
        // Still distinct events though
        assert_ne!(event1.event_id, event2.event_id);
    }

    #[test]
    fn csp_reports_with_different_directives_get_different_fingerprints() {
        let body1 =
            br#"{"csp-report":{"violated-directive":"script-src","blocked-uri":"https://a.com"}}"#;
        let body2 =
            br#"{"csp-report":{"violated-directive":"style-src","blocked-uri":"https://a.com"}}"#;
        let mut event1 = parse_security_body(body1, 3, &test_auth()).unwrap();
        let mut event2 = parse_security_body(body2, 3, &test_auth()).unwrap();
        crate::ingest::enrich::enrich_event(&mut event1);
        crate::ingest::enrich::enrich_event(&mut event2);
        assert_ne!(event1.fingerprint, event2.fingerprint);
    }

    #[test]
    fn parse_security_body_empty() {
        let result = parse_security_body(b"", 1, &test_auth());
        assert!(result.is_err());
    }

    // --- transaction extraction ---

    fn extract_txn(payload: &str) -> StorableEvent {
        let mut event = StorableEvent::new(
            String::new(),
            ItemType::Transaction,
            payload.as_bytes().to_vec(),
            1,
            "k".to_string(),
        );
        extract_fields(payload.as_bytes(), &ItemType::Transaction, &mut event, 0);
        event
    }

    #[test]
    fn transaction_duration_prefers_measurement() {
        let payload = r#"{"type":"transaction","transaction":"/api/health",
            "start_timestamp":1700000000.0,"timestamp":1700000002.0,
            "measurements":{"duration":{"value":1234.5,"unit":"millisecond"}},
            "contexts":{"trace":{"trace_id":"abc123","status":"ok"}}}"#;
        let event = extract_txn(payload);
        assert_eq!(event.duration_ms, Some(1235));
        assert_eq!(event.trace_id.as_deref(), Some("abc123"));
        assert_eq!(event.trace_status.as_deref(), Some("ok"));
    }

    #[test]
    fn transaction_duration_falls_back_to_timestamps() {
        let payload = r#"{"type":"transaction","transaction":"/api/slow",
            "start_timestamp":1700000000.0,"timestamp":1700000002.5,
            "contexts":{"trace":{"trace_id":"deadbeef","status":"internal_error"}}}"#;
        let event = extract_txn(payload);
        assert_eq!(event.duration_ms, Some(2500));
        assert_eq!(event.trace_id.as_deref(), Some("deadbeef"));
        assert_eq!(event.trace_status.as_deref(), Some("internal_error"));
    }

    #[test]
    fn non_transaction_gets_trace_id_but_no_perf_fields() {
        let payload = r#"{"message":"hello","contexts":{"trace":{"trace_id":"x"}}}"#;
        let mut event = StorableEvent::new(
            String::new(),
            ItemType::Event,
            payload.as_bytes().to_vec(),
            1,
            "k".to_string(),
        );
        extract_fields(payload.as_bytes(), &ItemType::Event, &mut event, 0);
        assert_eq!(event.trace_id.as_deref(), Some("x"));
        assert!(event.duration_ms.is_none());
        assert!(event.trace_status.is_none());
    }

    // --- session extraction ---

    fn extract_session(payload: &str) -> StorableEvent {
        let mut event = StorableEvent::new(
            String::new(),
            ItemType::Session,
            payload.as_bytes().to_vec(),
            1,
            "k".to_string(),
        );
        extract_fields(payload.as_bytes(), &ItemType::Session, &mut event, 0);
        event
    }

    #[test]
    fn session_reads_nested_release_and_environment() {
        let payload = r#"{"sid":"s1","did":"u1","init":true,"status":"ok","errors":0,
            "attrs":{"release":"app@1.0","environment":"prod"}}"#;
        let event = extract_session(payload);
        assert_eq!(event.session_buckets.len(), 1);
        let b = &event.session_buckets[0];
        assert_eq!(b.release, "app@1.0");
        assert_eq!(b.environment, "prod");
        assert_eq!(b.did.as_deref(), Some("u1"));
    }

    #[test]
    fn session_classifies_crashed() {
        let payload = r#"{"sid":"s1","init":true,"status":"crashed","errors":1,"attrs":{}}"#;
        let b = &extract_session(payload).session_buckets[0];
        assert_eq!(b.crashed, 1);
        assert_eq!(b.errored, 0);
        assert_eq!(b.abnormal, 0);
        assert_eq!(b.total, 1);
    }

    #[test]
    fn session_classifies_errored_when_errors_positive_and_status_ok() {
        let payload = r#"{"sid":"s1","init":true,"status":"ok","errors":2,"attrs":{}}"#;
        let b = &extract_session(payload).session_buckets[0];
        assert_eq!(b.errored, 1);
        assert_eq!(b.crashed, 0);
        assert_eq!(b.abnormal, 0);
    }

    #[test]
    fn session_classifies_abnormal() {
        let payload = r#"{"sid":"s1","init":true,"status":"abnormal","errors":0,"attrs":{}}"#;
        let b = &extract_session(payload).session_buckets[0];
        assert_eq!(b.abnormal, 1);
        assert_eq!(b.crashed, 0);
        assert_eq!(b.errored, 0);
    }

    #[test]
    fn session_healthy_has_no_failure_counts() {
        let payload = r#"{"sid":"s1","init":true,"status":"exited","errors":0,"attrs":{}}"#;
        let b = &extract_session(payload).session_buckets[0];
        assert_eq!(b.crashed, 0);
        assert_eq!(b.errored, 0);
        assert_eq!(b.abnormal, 0);
        assert_eq!(b.total, 1);
    }

    #[test]
    fn session_total_only_counted_on_init() {
        // Terminal crash update without init: still counts the crash, but not total.
        let payload = r#"{"sid":"s1","init":false,"status":"crashed","errors":1,"attrs":{}}"#;
        let b = &extract_session(payload).session_buckets[0];
        assert_eq!(b.total, 0);
        assert_eq!(b.crashed, 1);
    }

    #[test]
    fn session_started_ts_parsed_from_rfc3339() {
        // The session's own start time drives day bucketing; an rfc3339 string
        // must be parsed rather than collapsing onto the ingest timestamp.
        let payload = r#"{"sid":"s1","init":true,"status":"ok","errors":0,
            "started":"2025-03-07T12:00:00.000Z","attrs":{}}"#;
        let b = &extract_session(payload).session_buckets[0];
        assert_eq!(b.started_ts, 1_741_348_800); // 2025-03-07T12:00:00Z
    }

    #[test]
    fn aggregate_sessions_parses_multiple_entries() {
        let payload = r#"{
            "aggregates":[
                {"started":"2025-03-07T12:00:00.000Z","exited":100,"errored":5,"crashed":2},
                {"started":"2025-03-07T13:00:00.000Z","exited":50,"errored":0,"crashed":0}
            ],
            "attrs":{"release":"app@2.0","environment":"staging"}
        }"#;
        let mut event = StorableEvent::new(
            String::new(),
            ItemType::Sessions,
            payload.as_bytes().to_vec(),
            1,
            "k".to_string(),
        );
        extract_fields(payload.as_bytes(), &ItemType::Sessions, &mut event, 0);
        assert_eq!(event.session_buckets.len(), 2);
        let first = &event.session_buckets[0];
        assert_eq!(first.release, "app@2.0");
        assert_eq!(first.environment, "staging");
        assert_eq!(first.total, 107); // 100 + 5 + 2
        assert_eq!(first.crashed, 2);
        assert_eq!(first.errored, 5);
        assert!(first.is_aggregate);
        assert!(first.did.is_none());
        let second = &event.session_buckets[1];
        assert_eq!(second.total, 50);
        assert_eq!(second.crashed, 0);
    }

    // --- timestamp unit normalization ---

    fn ts_event(json_ts: &str) -> i64 {
        let payload = format!(r#"{{"timestamp":{json_ts}}}"#);
        let mut event = StorableEvent::new(
            String::new(),
            ItemType::Event,
            payload.as_bytes().to_vec(),
            1,
            "k".to_string(),
        );
        extract_fields(payload.as_bytes(), &ItemType::Event, &mut event, 0);
        event.timestamp
    }

    #[test]
    fn timestamp_units_normalized_for_2026_era_values() {
        let expected = 1_780_000_000;
        assert_eq!(ts_event("1780000000"), expected); // seconds
        assert_eq!(ts_event("1780000000.0"), expected);
        assert_eq!(ts_event("1780000000000"), expected); // milliseconds
        assert_eq!(ts_event("1780000000000.0"), expected);
        assert_eq!(ts_event("1780000000000000"), expected); // microseconds
        assert_eq!(ts_event("1780000000000000.0"), expected);
        assert_eq!(ts_event("1780000000000000000"), expected); // nanoseconds
        assert_eq!(ts_event("1.78e18"), expected);
    }

    // --- clock drift ---

    #[test]
    fn clock_drift_corrects_session_started_ts() {
        // started = 2026-01-01T00:00:00Z
        let started_ts = 1_767_225_600i64;
        let sent_at = (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        let body = format!(
            "{{\"sent_at\":\"{sent_at}\"}}\n{{\"type\":\"session\"}}\n{{\"sid\":\"s1\",\"init\":true,\"status\":\"ok\",\"errors\":0,\"started\":\"2026-01-01T00:00:00Z\",\"attrs\":{{}}}}\n"
        );
        let result = parse(body.as_bytes(), 1, &test_auth()).unwrap();
        let drift = result.clock_drift_secs;
        assert!((7195..=7205).contains(&drift), "drift={drift}");
        let b = &result.events[0].session_buckets[0];
        assert_eq!(
            b.started_ts,
            started_ts + drift,
            "session bucket start must get the same drift correction as the event"
        );
    }

    /// Envelope with a 2h-stale `sent_at` (so drift ≈ +7200) around one item.
    fn drifted_envelope(item_type: &str, payload: &str) -> ParsedEnvelope {
        let sent_at = (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        let body =
            format!("{{\"sent_at\":\"{sent_at}\"}}\n{{\"type\":\"{item_type}\"}}\n{payload}\n");
        let result = parse(body.as_bytes(), 1, &test_auth()).unwrap();
        assert!(
            (7195..=7205).contains(&result.clock_drift_secs),
            "drift={}",
            result.clock_drift_secs
        );
        result
    }

    #[test]
    fn clock_drift_corrects_client_supplied_event_timestamp() {
        let result = drifted_envelope("event", r#"{"event_id":"e1","timestamp":1780000000}"#);
        assert_eq!(
            result.events[0].timestamp,
            1_780_000_000 + result.clock_drift_secs
        );
    }

    // Regression: an event with no payload timestamp still holds the server-side
    // Utc::now() default, so drift correction would push it into the future.
    #[test]
    fn clock_drift_leaves_server_timestamp_alone() {
        let before = chrono::Utc::now().timestamp();
        let result = drifted_envelope("event", r#"{"event_id":"e1","message":"hi"}"#);
        let ts = result.events[0].timestamp;
        assert!(
            (before..=chrono::Utc::now().timestamp()).contains(&ts),
            "server timestamp must not be drifted: ts={ts}, now={before}"
        );
    }

    // Log batches never reach the timestamp block, so their event timestamp is
    // always server-side; only the per-entry client timestamps carry drift.
    #[test]
    fn clock_drift_applies_to_log_entries_not_the_batch() {
        let before = chrono::Utc::now().timestamp();
        let result = drifted_envelope(
            "log",
            r#"{"items":[{"body":"a","timestamp":1780000000},{"body":"b"}]}"#,
        );
        let event = &result.events[0];
        assert!((before..=chrono::Utc::now().timestamp()).contains(&event.timestamp));
        let entries = event.log_entries.as_ref().unwrap();
        assert_eq!(
            entries[0].timestamp,
            Some(1_780_000_000 + result.clock_drift_secs)
        );
        assert_eq!(
            entries[1].timestamp, None,
            "an entry without its own timestamp falls back to the event's at write time"
        );
    }

    #[test]
    fn clock_drift_corrects_embedded_span_timestamps() {
        let result = drifted_envelope(
            "transaction",
            r#"{"transaction":"/t","timestamp":1700000001.0,"contexts":{"trace":{"trace_id":"tr1"}},"spans":[{"span_id":"c1","trace_id":"tr1","op":"db","start_timestamp":1700000000.0,"timestamp":1700000000.5}]}"#,
        );
        let drift = result.clock_drift_secs;
        let span = &result.events[0].embedded_spans.as_ref().unwrap()[0];
        assert_eq!(span.timestamp, Some(1_700_000_001 + drift));
        assert_eq!(span.fields.start_ms, Some(1_700_000_000_000 + drift * 1000));
    }

    #[test]
    fn clock_drift_corrects_standalone_span_start_ms() {
        let result = drifted_envelope(
            "span",
            r#"{"span_id":"sp1","trace_id":"tr","op":"http.client","start_timestamp":1700000000.0,"timestamp":1700000000.25}"#,
        );
        let fields = result.events[0].span_fields.as_ref().unwrap();
        assert_eq!(
            fields.start_ms,
            Some(1_700_000_000_000 + result.clock_drift_secs * 1000)
        );
        assert_eq!(fields.duration_ms, Some(250), "durations are deltas");
    }

    // --- parse-time pre-extraction (spans, logs) ---

    #[test]
    fn transaction_precomputes_embedded_spans() {
        let payload = r#"{"type":"transaction","transaction":"/t",
            "contexts":{"trace":{"trace_id":"tr1"}},
            "spans":[
                {"span_id":"c1","trace_id":"tr1","op":"db",
                 "start_timestamp":1700000000.0,"timestamp":1700000000.5},
                {"op":"no-id"}
            ]}"#;
        let event = extract_txn(payload);
        let spans = event.embedded_spans.as_ref().unwrap();
        assert_eq!(spans.len(), 1, "spans without span_id are skipped");
        assert_eq!(spans[0].fields.span_id.as_deref(), Some("c1"));
        assert_eq!(spans[0].fields.op.as_deref(), Some("db"));
        assert_eq!(spans[0].timestamp, Some(1_700_000_001));
    }

    #[test]
    fn standalone_span_precomputes_fields() {
        let payload = r#"{"span_id":"sp1","trace_id":"tr","op":"http.client",
            "start_timestamp":1700000000.0,"timestamp":1700000000.25}"#;
        let mut event = StorableEvent::new(
            String::new(),
            ItemType::Span,
            payload.as_bytes().to_vec(),
            1,
            "k".to_string(),
        );
        extract_fields(payload.as_bytes(), &ItemType::Span, &mut event, 0);
        let f = event.span_fields.as_ref().unwrap();
        assert_eq!(f.span_id.as_deref(), Some("sp1"));
        assert_eq!(f.trace_id.as_deref(), Some("tr"));
        assert_eq!(f.duration_ms, Some(250));
        assert_eq!(f.start_ms, Some(1_700_000_000_000));
    }

    #[test]
    fn log_batch_precomputes_entries() {
        let payload = r#"{"items":[
            {"body":"first","level":"warn","timestamp":1780000000,"trace_id":"t1"},
            {"body":"second","level":"error"}
        ]}"#;
        let mut event = StorableEvent::new(
            String::new(),
            ItemType::Log,
            payload.as_bytes().to_vec(),
            1,
            "k".to_string(),
        );
        extract_fields(payload.as_bytes(), &ItemType::Log, &mut event, 0);
        let entries = event.log_entries.as_ref().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].fields.body.as_deref(), Some("first"));
        assert_eq!(entries[0].fields.level.as_deref(), Some("warning"));
        assert_eq!(entries[0].fields.trace_id.as_deref(), Some("t1"));
        assert_eq!(entries[0].timestamp, Some(1_780_000_000));
        let round: Value = serde_json::from_slice(&entries[1].payload).unwrap();
        assert_eq!(round.get("body").and_then(|v| v.as_str()), Some("second"));
    }

    #[test]
    fn single_log_object_precomputes_one_entry() {
        let payload = r#"{"body":"only","level":"info"}"#;
        let mut event = StorableEvent::new(
            String::new(),
            ItemType::Log,
            payload.as_bytes().to_vec(),
            1,
            "k".to_string(),
        );
        extract_fields(payload.as_bytes(), &ItemType::Log, &mut event, 0);
        let entries = event.log_entries.as_ref().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].fields.body.as_deref(), Some("only"));
        assert_eq!(entries[0].fields.level.as_deref(), Some("info"));
    }

    // --- title enrichment (via parse_store_body + enrich_event) ---

    #[test]
    fn title_from_exception() {
        let body =
            br#"{"exception":{"values":[{"type":"TypeError","value":"null is not an object"}]}}"#;
        let mut event = parse_store_body(body, 1, &test_auth()).unwrap();
        crate::ingest::enrich::enrich_event(&mut event);
        assert_eq!(
            event.title.as_deref(),
            Some("TypeError: null is not an object")
        );
    }

    #[test]
    fn title_from_exception_no_value() {
        let body = br#"{"exception":{"values":[{"type":"RuntimeError"}]}}"#;
        let mut event = parse_store_body(body, 1, &test_auth()).unwrap();
        crate::ingest::enrich::enrich_event(&mut event);
        assert_eq!(event.title.as_deref(), Some("RuntimeError"));
    }

    #[test]
    fn title_from_message_fallback() {
        let body = br#"{"message":"something broke"}"#;
        let mut event = parse_store_body(body, 1, &test_auth()).unwrap();
        crate::ingest::enrich::enrich_event(&mut event);
        assert_eq!(event.title.as_deref(), Some("something broke"));
    }

    #[test]
    fn title_from_logentry() {
        let body = br#"{"logentry":{"message":"log msg"}}"#;
        let mut event = parse_store_body(body, 1, &test_auth()).unwrap();
        crate::ingest::enrich::enrich_event(&mut event);
        assert_eq!(event.title.as_deref(), Some("log msg"));
    }

    #[test]
    fn title_from_transaction_fallback() {
        let body = br#"{"transaction":"/api/health"}"#;
        let mut event = parse_store_body(body, 1, &test_auth()).unwrap();
        crate::ingest::enrich::enrich_event(&mut event);
        assert_eq!(event.title.as_deref(), Some("/api/health"));
    }

    #[test]
    fn title_none_when_no_fields() {
        let body = br#"{"level":"info"}"#;
        let mut event = parse_store_body(body, 1, &test_auth()).unwrap();
        crate::ingest::enrich::enrich_event(&mut event);
        assert!(event.title.is_none());
    }
}
