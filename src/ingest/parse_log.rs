//! Structured-log payload parsing (OTEL-style entries) -> per-entry fields.

pub(crate) struct LogFields {
    pub level: Option<String>,
    pub body: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub attributes: Option<String>,
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

pub(crate) fn parse_log_entries(payload: &[u8]) -> Vec<serde_json::Value> {
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
pub(crate) fn compress_log_entry(entry: &serde_json::Value) -> Vec<u8> {
    let json_bytes = serde_json::to_vec(entry).unwrap_or_default();
    zstd::encode_all(std::io::Cursor::new(&json_bytes), 3).unwrap_or(json_bytes)
}
