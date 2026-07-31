//! Post-parse enrichment: fingerprinting and title extraction (business logic separate from parser).

use crate::ingest::fingerprint;
use crate::ingest::lenient::{lenient_field, message_text};
use crate::ingest::models::{ItemType, StorableEvent};
use serde_json::Value;

/// Parse the JSON payload and fill in `fingerprint` (if missing) and `title`.
/// Skips re-parsing when nothing is left to compute (e.g. after `extract_fields`).
pub fn enrich_event(event: &mut StorableEvent) {
    let need_fingerprint = event.fingerprint.is_none() && event.item_type.can_fingerprint();
    // Log batches carry no title fields; their entries were already extracted.
    let need_title =
        event.title.is_none() && !(event.item_type == ItemType::Log && event.log_entries.is_some());
    if !need_fingerprint && !need_title {
        return;
    }

    let json: Value = match serde_json::from_slice(&event.payload) {
        Ok(v) => v,
        Err(_) => return,
    };

    if need_fingerprint {
        event.fingerprint =
            fingerprint::compute_fingerprint_from_value(event.project_id, &event.item_type, &json);
    }
    if need_title {
        if let Some(title) = extract_title(&json, &event.item_type, event.monitor_slug.as_deref()) {
            event.title = Some(title);
        }
    }
}

/// Extract a title from pre-parsed JSON — exposed so `envelope::extract_fields`
/// can compute it without a second parse.
pub(crate) fn extract_title_from(
    json: &Value,
    item_type: &ItemType,
    monitor_slug: Option<&str>,
) -> Option<String> {
    extract_title(json, item_type, monitor_slug)
}

/// Pick a human-readable title: special cases (check-ins, sessions, user
/// reports), then exception > message/logentry > transaction name.
fn extract_title(json: &Value, item_type: &ItemType, monitor_slug: Option<&str>) -> Option<String> {
    if *item_type == ItemType::CheckIn {
        let slug = monitor_slug.or_else(|| json.get("monitor_slug").and_then(|v| v.as_str()));
        let status = json
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Some(match slug {
            Some(s) => format!("{s}: {status}"),
            None => format!("check_in: {status}"),
        });
    }

    if *item_type == ItemType::Session {
        let status = json
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Some(format!("session: {status}"));
    }

    if *item_type == ItemType::UserReport {
        let name = json.get("name").and_then(|v| v.as_str());
        let email = json.get("email").and_then(|v| v.as_str());
        return Some(match (name, email) {
            (Some(n), _) => format!("User Report from {n}"),
            (None, Some(e)) => format!("User Report from {e}"),
            _ => "User Report".to_string(),
        });
    }

    if let Some(exc) = json
        .get("exception")
        .and_then(|e| e.get("values"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
    {
        let exc_type = exc.get("type").and_then(|v| v.as_str()).unwrap_or("Error");
        let exc_value = lenient_field(exc, "value").unwrap_or_default();
        if exc_value.is_empty() {
            return Some(exc_type.to_string());
        }
        return Some(format!("{exc_type}: {exc_value}"));
    }

    if let Some(msg) = message_text(json) {
        return Some(msg.chars().take(200).collect());
    }

    if let Some(txn) = json.get("transaction").and_then(|v| v.as_str()) {
        return Some(txn.to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(json: &Value, item_type: ItemType) -> StorableEvent {
        let raw = serde_json::to_vec(json).unwrap();

        StorableEvent {
            event_id: "test-id".to_string(),
            item_type,
            payload: raw,
            compressed: false,
            project_id: 1,
            public_key: "testkey".to_string(),
            timestamp: 1000,
            level: None,
            platform: None,
            release: None,
            environment: None,
            server_name: None,
            transaction_name: None,
            title: None,
            sdk_name: None,
            sdk_version: None,
            fingerprint: None,
            monitor_slug: None,
            session_status: None,
            parent_event_id: None,
            user_identifier: None,
            tags: Vec::new(),
            session_buckets: Vec::new(),
            trace_id: None,
            duration_ms: None,
            trace_status: None,
            embedded_spans: None,
            span_fields: None,
            log_entries: None,
        }
    }

    #[test]
    fn enrich_sets_fingerprint_and_title() {
        let json = serde_json::json!({"message": "hello"});
        let mut event = make_event(&json, ItemType::Event);

        assert!(event.fingerprint.is_none());
        assert!(event.title.is_none());

        enrich_event(&mut event);

        assert!(event.fingerprint.is_some());
        assert_eq!(event.title.as_deref(), Some("hello"));
    }

    #[test]
    fn enrich_exception_title() {
        let json = serde_json::json!({
            "exception": {"values": [{"type": "TypeError", "value": "null is not an object"}]}
        });
        let mut event = make_event(&json, ItemType::Event);
        enrich_event(&mut event);
        assert_eq!(
            event.title.as_deref(),
            Some("TypeError: null is not an object")
        );
    }

    // The React Native console integration forwards the console argument list,
    // so `value` arrives as an array. Dropping it left every such event titled
    // with a bare "Error" and grouped into one issue.
    #[test]
    fn enrich_exception_title_from_array_value() {
        let json = serde_json::json!({
            "exception": {"values": [{"type": "Error", "value": ["+ STATE: Unknown consent status"]}]}
        });
        let mut event = make_event(&json, ItemType::Event);
        enrich_event(&mut event);
        assert_eq!(
            event.title.as_deref(),
            Some("Error: + STATE: Unknown consent status")
        );
    }

    #[test]
    fn enrich_title_from_object_valued_message() {
        let json = serde_json::json!({"message": {"message": ["consent missing"]}});
        let mut event = make_event(&json, ItemType::Event);
        enrich_event(&mut event);
        assert_eq!(event.title.as_deref(), Some("consent missing"));
    }

    #[test]
    fn enrich_no_fingerprint_for_session() {
        let json = serde_json::json!({"status": "ok"});
        let mut event = make_event(&json, ItemType::Session);
        enrich_event(&mut event);
        assert!(event.fingerprint.is_none());
        assert_eq!(event.title.as_deref(), Some("session: ok"));
    }

    #[test]
    fn enrich_transaction() {
        let json = serde_json::json!({"transaction": "/api/health"});
        let mut event = make_event(&json, ItemType::Transaction);
        enrich_event(&mut event);
        // transactions feed transaction_metrics, not issues: no fingerprint
        assert!(event.fingerprint.is_none());
        assert_eq!(event.title.as_deref(), Some("/api/health"));
    }

    #[test]
    fn enrich_logentry() {
        let json = serde_json::json!({"logentry": {"message": "log msg"}});
        let mut event = make_event(&json, ItemType::Event);
        enrich_event(&mut event);
        assert_eq!(event.title.as_deref(), Some("log msg"));
    }

    #[test]
    fn enrich_no_title_fields() {
        let json = serde_json::json!({"level": "info"});
        let mut event = make_event(&json, ItemType::Event);
        enrich_event(&mut event);
        assert!(event.title.is_none());
        // still fingerprinted: random UUID fallback
        assert!(event.fingerprint.is_some());
    }

    #[test]
    fn enrich_check_in() {
        let json = serde_json::json!({"status": "ok", "monitor_slug": "my-cron"});
        let mut event = make_event(&json, ItemType::CheckIn);
        event.monitor_slug = Some("my-cron".to_string());
        enrich_event(&mut event);
        assert_eq!(event.title.as_deref(), Some("my-cron: ok"));
        // check-ins don't produce issues, so no fingerprint
        assert!(event.fingerprint.is_none());
    }

    #[test]
    fn enrich_user_report() {
        let json = serde_json::json!({"name": "Alice", "email": "alice@example.com"});
        let mut event = make_event(&json, ItemType::UserReport);
        enrich_event(&mut event);
        assert_eq!(event.title.as_deref(), Some("User Report from Alice"));
    }

    #[test]
    fn enrich_titled_non_fingerprintable_skips_payload() {
        let json = serde_json::json!({"transaction": "/other"});
        let mut event = make_event(&json, ItemType::Transaction);
        event.title = Some("/api/health".to_string());
        enrich_event(&mut event);
        // nothing left to compute: no fingerprint for transactions, title kept
        assert!(event.fingerprint.is_none());
        assert_eq!(event.title.as_deref(), Some("/api/health"));
    }

    #[test]
    fn enrich_log_batch_skips_payload() {
        let json = serde_json::json!({"items": []});
        let mut event = make_event(&json, ItemType::Log);
        event.log_entries = Some(Vec::new());
        enrich_event(&mut event);
        assert!(event.fingerprint.is_none());
        assert!(event.title.is_none());
    }

    #[test]
    fn enrich_event_without_fingerprint_still_enriched() {
        let json = serde_json::json!({"message": "hello"});
        let mut event = make_event(&json, ItemType::Event);
        event.title = Some("preset".to_string());
        enrich_event(&mut event);
        assert!(event.fingerprint.is_some());
        assert_eq!(event.title.as_deref(), Some("preset"));
    }

    #[test]
    fn enrich_invalid_payload_is_noop() {
        let mut event = StorableEvent {
            event_id: "bad".to_string(),
            item_type: ItemType::Event,
            payload: vec![0, 1, 2, 3], // not valid JSON
            compressed: false,
            project_id: 1,
            public_key: "k".to_string(),
            timestamp: 0,
            level: None,
            platform: None,
            release: None,
            environment: None,
            server_name: None,
            transaction_name: None,
            title: None,
            sdk_name: None,
            sdk_version: None,
            fingerprint: None,
            monitor_slug: None,
            session_status: None,
            parent_event_id: None,
            user_identifier: None,
            tags: Vec::new(),
            session_buckets: Vec::new(),
            trace_id: None,
            duration_ms: None,
            trace_status: None,
            embedded_spans: None,
            span_fields: None,
            log_entries: None,
        };
        enrich_event(&mut event);
        assert!(event.fingerprint.is_none());
        assert!(event.title.is_none());
    }
}
