//! Span payload parsing -> normalized span fields.

pub(super) struct SpanFields {
    pub span_id: Option<String>,
    pub trace_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub op: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub duration_ms: Option<i64>,
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

pub(super) fn extract_span_fields(payload: &[u8]) -> SpanFields {
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
