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
                .map(String::from);

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
                tracing::warn!(
                    "envelope item declared length {len_u64} exceeds remaining body ({} bytes), truncating",
                    body.len() - pos
                );
                let slice = &body[pos..];
                pos = body.len();
                slice
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

        let parsed_event_id = extract_fields(payload_bytes, &item_type, &mut event);

        if result.clock_drift_secs != 0 {
            event.timestamp += result.clock_drift_secs;
        }

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
    extract_fields(payload, item_type, event);
}

/// Pull known fields out of the JSON payload into a StorableEvent.
/// Returns the event_id if one was present.
fn extract_fields(
    payload: &[u8],
    item_type: &ItemType,
    event: &mut StorableEvent,
) -> Option<String> {
    let json: Value = match serde_json::from_slice(payload) {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Log items may arrive as a JSON array or {"items": [...]} batch. Per-item
    // fields are extracted downstream in parse_log_entries; return None here so
    // the event gets a generated UUID.
    if *item_type == ItemType::Log
        && (json.is_array() || json.get("items").and_then(|v| v.as_array()).is_some())
    {
        return None;
    }

    let event_id = json
        .get("event_id")
        .and_then(|v| v.as_str())
        .map(String::from);

    if let Some(ts) = json.get("timestamp").and_then(|v| {
        v.as_f64()
            .filter(|f| f.is_finite())
            .map(|f| {
                if f > 1e15 {
                    (f / 1_000_000_000.0).round() as i64
                } else if f > 1e12 {
                    (f / 1_000.0).round() as i64
                } else {
                    f.round() as i64
                }
            })
            .or_else(|| {
                v.as_i64().map(|i| {
                    if i > 1_000_000_000_000_000 {
                        i / 1_000_000_000
                    } else if i > 1_000_000_000_000 {
                        i / 1_000
                    } else {
                        i
                    }
                })
            })
    }) {
        event.timestamp = ts;
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
        extract_session_bucket(&json, event);
    } else if *item_type == ItemType::Sessions {
        extract_session_aggregates(&json, event);
    } else if *item_type == ItemType::Transaction {
        extract_transaction_perf(&json, event);
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
        u.get("id")
            .and_then(|v| {
                v.as_str()
                    .map(String::from)
                    .or_else(|| v.as_u64().map(|n| n.to_string()))
            })
            .or_else(|| u.get("email").and_then(|v| v.as_str()).map(String::from))
            .or_else(|| u.get("username").and_then(|v| v.as_str()).map(String::from))
            .or_else(|| {
                u.get("ip_address")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
    });

    event.tags = extract_tags_from_json(&json);

    // Compute fingerprint and title from the already-parsed JSON so
    // enrich_event won't need to re-parse the payload
    event.fingerprint = crate::ingest::fingerprint::compute_fingerprint_from_value(
        event.project_id,
        item_type,
        &json,
    );
    event.title =
        crate::ingest::enrich::extract_title_from(&json, item_type, event.monitor_slug.as_deref());

    event_id
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
fn extract_session_bucket(json: &Value, event: &mut StorableEvent) {
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
        .map_or(event.timestamp, |dt| dt.timestamp());

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
fn extract_session_aggregates(json: &Value, event: &mut StorableEvent) {
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
            .map_or(event.timestamp, |dt| dt.timestamp());

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
fn extract_tags_from_json(json: &Value) -> Vec<(String, String)> {
    let tags = match json.get("tags") {
        Some(v) => v,
        None => return Vec::new(),
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

    let event_id = extract_fields(body, &ItemType::Event, &mut event);
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
        extract_fields(payload.as_bytes(), &ItemType::Transaction, &mut event);
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
        extract_fields(payload.as_bytes(), &ItemType::Event, &mut event);
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
        extract_fields(payload.as_bytes(), &ItemType::Session, &mut event);
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
        extract_fields(payload.as_bytes(), &ItemType::Sessions, &mut event);
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
