//! Structured-log payload parsing (OTEL-style entries) -> per-entry fields.

use crate::ingest::ids::sanitize_id;
use crate::ingest::text::{clean_text, strip_nul};

#[derive(Debug, Clone, PartialEq)]
pub struct LogFields {
    pub level: Option<String>,
    pub body: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub attributes: Option<String>,
}

/// One log entry pre-extracted at envelope-parse time, so the writer flush
/// doesn't decompress and re-parse the batch payload.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLogEntry {
    /// Serialized entry JSON, zstd-compressed per entry once `compressed` is set.
    pub payload: Vec<u8>,
    pub compressed: bool,
    /// The entry's own timestamp; falls back to the event's at flush.
    pub timestamp: Option<i64>,
    pub fields: LogFields,
}

pub fn parse_log_entry(v: &serde_json::Value) -> ParsedLogEntry {
    ParsedLogEntry {
        payload: serde_json::to_vec(v).unwrap_or_default(),
        compressed: false,
        timestamp: extract_log_timestamp(v),
        fields: extract_log_fields(v),
    }
}

const MAX_LOG_ENTRIES: usize = 10_000;

fn normalize_log_level(level: &str) -> String {
    match level.to_ascii_lowercase().as_str() {
        "warn" | "warning" => "warning".to_string(),
        "err" => "error".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn extract_log_timestamp(v: &serde_json::Value) -> Option<i64> {
    v.get("timestamp").and_then(|ts| {
        if let Some(f) = ts.as_f64() {
            // magnitude distinguishes ns / us / ms / s, normalised to seconds
            if f > 1e18 {
                Some((f / 1e9) as i64)
            } else if f > 1e15 {
                Some((f / 1e6) as i64)
            } else if f > 1e12 {
                Some((f / 1e3) as i64)
            } else {
                Some(f as i64)
            }
        } else {
            ts.as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|f| f as i64)
        }
    })
}

pub(crate) fn extract_log_fields(v: &serde_json::Value) -> LogFields {
    LogFields {
        level: v
            .get("level")
            .or_else(|| v.get("severity_text"))
            .and_then(|v| v.as_str())
            .map(|s| normalize_log_level(&clean_text(s))),
        body: v
            .get("body")
            .or_else(|| v.get("message"))
            .and_then(|v| {
                v.as_str()
                    .or_else(|| v.get("string_value").and_then(|sv| sv.as_str()))
            })
            .map(strip_nul),
        trace_id: v
            .get("trace_id")
            .and_then(|v| v.as_str())
            .and_then(sanitize_id),
        span_id: v
            .get("span_id")
            .and_then(|v| v.as_str())
            .and_then(sanitize_id),
        attributes: v.get("attributes").map(|a| a.to_string()),
    }
}

pub fn parse_log_entries(payload: &[u8]) -> Vec<serde_json::Value> {
    let json: Option<serde_json::Value> = zstd::decode_all(std::io::Cursor::new(payload))
        .ok()
        .or_else(|| Some(payload.to_vec()))
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    match json {
        Some(json) => log_entries_from_value(json),
        None => Vec::new(),
    }
}

/// Split an already-parsed log payload into its entries: a bare array or a
/// Sentry SDK `{"items": [...]}` batch; anything else is one entry.
pub fn log_entries_from_value(json: serde_json::Value) -> Vec<serde_json::Value> {
    use serde_json::Value;
    match json {
        Value::Array(mut arr) => {
            arr.truncate(MAX_LOG_ENTRIES);
            arr
        }
        Value::Object(mut map) => match map.remove("items") {
            Some(Value::Array(mut items)) => {
                items.truncate(MAX_LOG_ENTRIES);
                items
            }
            Some(other) => {
                map.insert("items".to_string(), other);
                vec![Value::Object(map)]
            }
            None => vec![Value::Object(map)],
        },
        other => vec![other],
    }
}

/// Compress a single log entry to its own zstd blob.
pub(crate) fn compress_log_entry(entry: &serde_json::Value) -> Vec<u8> {
    let json_bytes = serde_json::to_vec(entry).unwrap_or_default();
    zstd::encode_all(std::io::Cursor::new(&json_bytes), 3).unwrap_or(json_bytes)
}

/// Compress pre-serialized log entry bytes to their own zstd blob.
pub(crate) fn compress_log_bytes(json_bytes: &[u8]) -> Vec<u8> {
    zstd::encode_all(std::io::Cursor::new(json_bytes), 3).unwrap_or_else(|_| json_bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::text::MAX_TEXT_CHARS;

    #[test]
    fn log_text_and_ids_are_sanitized() {
        let long = "x".repeat(5000);
        let raw = format!(
            r#"{{"level":"WARN\u0000{long}","body":"disk\u0000 full {long}","trace_id":"{}","span_id":"ab\u0000cd"}}"#,
            "a".repeat(65)
        );
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let f = extract_log_fields(&v);
        let level = f.level.unwrap();
        assert_eq!(level.chars().count(), MAX_TEXT_CHARS);
        assert!(level.starts_with("warn") && !level.contains('\0'));
        assert_eq!(f.body, Some(format!("disk full {long}")));
        assert_eq!(f.trace_id, None);
        assert_eq!(f.span_id, None);

        let v = serde_json::json!({
            "level": "WARN",
            "body": "disk full",
            "trace_id": "0af7651916cd43dd8448eb211c80319c",
            "span_id": "b7ad6b7169203331",
        });
        let f = extract_log_fields(&v);
        assert_eq!(f.level.as_deref(), Some("warning"));
        assert_eq!(f.body.as_deref(), Some("disk full"));
        assert_eq!(
            f.trace_id.as_deref(),
            Some("0af7651916cd43dd8448eb211c80319c")
        );
        assert_eq!(f.span_id.as_deref(), Some("b7ad6b7169203331"));
    }
}
