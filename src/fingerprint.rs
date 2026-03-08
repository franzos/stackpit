//! Fingerprinting for issue grouping.
//!
//! The thing is, Sentry groups events into issues by fingerprint — and the
//! priority chain matters: SDK-provided fingerprint > exception type+value >
//! log message template > transaction name > random UUID. Each fingerprint
//! is scoped by project_id so projects don't bleed into each other.

use crate::models::ItemType;
use serde_json::Value;

/// FNV-1a 64-bit — fast, deterministic, and good enough for fingerprinting.
pub(crate) fn fnv1a_64(data: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x00000100000001B3;

    let mut hash = FNV_OFFSET_BASIS;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Zero-padded 16-char hex string from a 64-bit hash.
fn format_hash(hash: u64) -> String {
    format!("{:016x}", hash)
}

/// Fingerprint from an already-parsed JSON value.
/// Returns `None` for item types that don't produce issues — sessions, client reports, etc.
pub fn compute_fingerprint_from_value(
    project_id: u64,
    item_type: &ItemType,
    json: &Value,
) -> Option<String> {
    match item_type {
        ItemType::Event | ItemType::Transaction => {}
        _ => return None,
    }

    compute_fingerprint_inner(project_id, json)
}

/// Fingerprint from raw JSON bytes.
/// Returns `None` for non-issue item types. Falls back to a random UUID
/// if the JSON can't be parsed — better to store an ungrouped event than drop it.
pub fn compute_fingerprint(
    project_id: u64,
    item_type: &ItemType,
    payload_json: &[u8],
) -> Option<String> {
    match item_type {
        ItemType::Event | ItemType::Transaction => {}
        _ => return None,
    }

    let json: Value = match serde_json::from_slice(payload_json) {
        Ok(v) => v,
        Err(_) => return Some(uuid::Uuid::new_v4().to_string()),
    };

    compute_fingerprint_inner(project_id, &json)
}

fn compute_fingerprint_inner(project_id: u64, json: &Value) -> Option<String> {
    // SDK-provided fingerprint array wins — unless it's just ["{{ default }}"]
    if let Some(fp_array) = json.get("fingerprint").and_then(|v| v.as_array()) {
        let is_default_only = fp_array.len() == 1 && fp_array[0].as_str() == Some("{{ default }}");

        if !is_default_only && !fp_array.is_empty() {
            let mut input = Vec::new();
            input.extend_from_slice(&project_id.to_be_bytes());
            for (i, elem) in fp_array.iter().enumerate() {
                if i > 0 {
                    input.push(0x00);
                }
                if let Some(s) = elem.as_str() {
                    input.extend_from_slice(s.as_bytes());
                } else {
                    // Non-string elements get their JSON repr — shouldn't happen often
                    input.extend_from_slice(elem.to_string().as_bytes());
                }
            }
            return Some(format_hash(fnv1a_64(&input)));
        }
    }

    // Exception type+value, scoped by project — the null bytes prevent collisions
    if let Some(exc) = json
        .get("exception")
        .and_then(|e| e.get("values"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
    {
        let exc_type = exc.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let exc_value = exc.get("value").and_then(|v| v.as_str()).unwrap_or("");

        let mut input = Vec::new();
        input.extend_from_slice(project_id.to_string().as_bytes());
        input.push(0x00);
        input.extend_from_slice(exc_type.as_bytes());
        input.push(0x00);
        input.extend_from_slice(exc_value.as_bytes());
        return Some(format_hash(fnv1a_64(&input)));
    }

    // Message template — logentry.message is the unformatted template, which is
    // what we want for grouping. Top-level `message` is the fallback.
    let logentry_msg = json
        .get("logentry")
        .and_then(|l| l.get("message"))
        .and_then(|v| v.as_str());
    let top_msg = json.get("message").and_then(|v| v.as_str());

    if let Some(msg) = logentry_msg.or(top_msg) {
        let mut input = Vec::new();
        input.extend_from_slice(project_id.to_string().as_bytes());
        input.push(0x00);
        input.extend_from_slice(msg.as_bytes());
        return Some(format_hash(fnv1a_64(&input)));
    }

    // Transaction name — last structured option before we give up
    if let Some(txn) = json.get("transaction").and_then(|v| v.as_str()) {
        let mut input = Vec::new();
        input.extend_from_slice(project_id.to_string().as_bytes());
        input.push(0x00);
        input.extend_from_slice(txn.as_bytes());
        return Some(format_hash(fnv1a_64(&input)));
    }

    // Nothing to group on — random UUID, each event becomes its own issue
    Some(uuid::Uuid::new_v4().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_known_value() {
        // Empty string should give back the offset basis — it's the known starting point
        assert_eq!(fnv1a_64(b""), 0xcbf29ce484222325);
    }

    #[test]
    fn format_hash_zero_padded() {
        assert_eq!(format_hash(0), "0000000000000000");
        assert_eq!(format_hash(255), "00000000000000ff");
        assert_eq!(format_hash(0xdeadbeefcafebabe), "deadbeefcafebabe");
    }

    #[test]
    fn non_event_types_return_none() {
        let payload = br#"{"message":"hello"}"#;
        assert!(compute_fingerprint(1, &ItemType::Session, payload).is_none());
        assert!(compute_fingerprint(1, &ItemType::Sessions, payload).is_none());
        assert!(compute_fingerprint(1, &ItemType::ClientReport, payload).is_none());
        assert!(compute_fingerprint(1, &ItemType::Attachment, payload).is_none());
        assert!(compute_fingerprint(1, &ItemType::CheckIn, payload).is_none());
        assert!(compute_fingerprint(1, &ItemType::Profile, payload).is_none());
        assert!(compute_fingerprint(1, &ItemType::ReplayEvent, payload).is_none());
        assert!(compute_fingerprint(1, &ItemType::ReplayRecording, payload).is_none());
        assert!(compute_fingerprint(1, &ItemType::UserReport, payload).is_none());
        assert!(compute_fingerprint(1, &ItemType::Unknown, payload).is_none());
    }

    #[test]
    fn event_and_transaction_return_some() {
        let payload = br#"{"message":"hello"}"#;
        assert!(compute_fingerprint(1, &ItemType::Event, payload).is_some());
        assert!(compute_fingerprint(1, &ItemType::Transaction, payload).is_some());
    }

    #[test]
    fn custom_fingerprint_array() {
        let payload = br#"{"fingerprint":["my-custom-group","extra"],"message":"hello"}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();
        // 16-char hex from hashing the concatenated fingerprint parts
        assert_eq!(fp.len(), 16);

        // Same custom fingerprint on a different project — should NOT collide
        let payload2 = br#"{"fingerprint":["my-custom-group","extra"],"message":"different"}"#;
        let fp2 = compute_fingerprint(999, &ItemType::Event, payload2).unwrap();
        assert_ne!(fp, fp2);

        // Same custom fingerprint on the same project — should be deterministic
        let fp3 = compute_fingerprint(1, &ItemType::Event, payload).unwrap();
        assert_eq!(fp, fp3);
    }

    #[test]
    fn default_fingerprint_falls_through() {
        // ["{{ default }}"] means "use normal grouping" — it shouldn't override
        let payload = br#"{"fingerprint":["{{ default }}"],"message":"hello"}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();

        // Should fall through to message-based fingerprinting
        let payload_no_fp = br#"{"message":"hello"}"#;
        let fp_no_fp = compute_fingerprint(1, &ItemType::Event, payload_no_fp).unwrap();
        assert_eq!(fp, fp_no_fp);
    }

    #[test]
    fn exception_fingerprint() {
        let payload =
            br#"{"exception":{"values":[{"type":"TypeError","value":"null is not an object"}]}}"#;
        let fp = compute_fingerprint(42, &ItemType::Event, payload).unwrap();
        assert_eq!(fp.len(), 16);

        // Same exception on same project — should be deterministic
        let fp2 = compute_fingerprint(42, &ItemType::Event, payload).unwrap();
        assert_eq!(fp, fp2);

        // Different project — shouldn't collide
        let fp3 = compute_fingerprint(43, &ItemType::Event, payload).unwrap();
        assert_ne!(fp, fp3);
    }

    #[test]
    fn chained_exceptions_uses_first() {
        let payload = br#"{"exception":{"values":[
            {"type":"ValueError","value":"bad value"},
            {"type":"RuntimeError","value":"runtime issue"}
        ]}}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();

        // We only look at the first exception in the chain for grouping
        let payload_single =
            br#"{"exception":{"values":[{"type":"ValueError","value":"bad value"}]}}"#;
        let fp_single = compute_fingerprint(1, &ItemType::Event, payload_single).unwrap();
        assert_eq!(fp, fp_single);
    }

    #[test]
    fn message_fingerprint() {
        let payload = br#"{"message":"something broke"}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();
        assert_eq!(fp.len(), 16);

        // Deterministic — same message, same project, same result
        let fp2 = compute_fingerprint(1, &ItemType::Event, payload).unwrap();
        assert_eq!(fp, fp2);

        // Different message — different fingerprint
        let payload2 = br#"{"message":"something else broke"}"#;
        let fp3 = compute_fingerprint(1, &ItemType::Event, payload2).unwrap();
        assert_ne!(fp, fp3);
    }

    #[test]
    fn logentry_template_preferred_over_formatted() {
        // logentry.message is the unformatted template — that's what we group on
        let payload =
            br#"{"logentry":{"message":"User %s logged in","formatted":"User alice logged in"}}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();

        // Different rendered value but same template — should still group together
        let payload2 =
            br#"{"logentry":{"message":"User %s logged in","formatted":"User bob logged in"}}"#;
        let fp2 = compute_fingerprint(1, &ItemType::Event, payload2).unwrap();
        assert_eq!(fp, fp2);
    }

    #[test]
    fn logentry_preferred_over_top_level_message() {
        let payload = br#"{"logentry":{"message":"template %s"},"message":"rendered value"}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();

        // logentry.message takes priority over top-level message
        let payload_logentry_only = br#"{"logentry":{"message":"template %s"}}"#;
        let fp2 = compute_fingerprint(1, &ItemType::Event, payload_logentry_only).unwrap();
        assert_eq!(fp, fp2);
    }

    #[test]
    fn transaction_fingerprint() {
        let payload = br#"{"transaction":"/api/health","type":"transaction"}"#;
        let fp = compute_fingerprint(1, &ItemType::Transaction, payload).unwrap();
        assert_eq!(fp.len(), 16);

        // Deterministic
        let fp2 = compute_fingerprint(1, &ItemType::Transaction, payload).unwrap();
        assert_eq!(fp, fp2);
    }

    #[test]
    fn fallback_uuid_for_empty_event() {
        let payload = br#"{"level":"info"}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();
        // UUID fallback — 36 chars, dashes included
        assert_eq!(fp.len(), 36);
        assert!(fp.contains('-'));
    }

    #[test]
    fn invalid_json_gives_uuid_fallback() {
        let payload = b"not json at all";
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();
        assert_eq!(fp.len(), 36);
    }

    #[test]
    fn null_separator_prevents_ambiguity() {
        // The null byte separator prevents "TypeError" + "" from colliding with "Type" + "Error"
        let payload1 = br#"{"exception":{"values":[{"type":"TypeError","value":""}]}}"#;
        let payload2 = br#"{"exception":{"values":[{"type":"Type","value":"Error"}]}}"#;

        let fp1 = compute_fingerprint(1, &ItemType::Event, payload1).unwrap();
        let fp2 = compute_fingerprint(1, &ItemType::Event, payload2).unwrap();
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn exception_takes_priority_over_message() {
        let payload =
            br#"{"exception":{"values":[{"type":"TypeError","value":"bad"}]},"message":"hello"}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();

        // Exception wins over message in the priority chain
        let payload_exc_only = br#"{"exception":{"values":[{"type":"TypeError","value":"bad"}]}}"#;
        let fp_exc = compute_fingerprint(1, &ItemType::Event, payload_exc_only).unwrap();
        assert_eq!(fp, fp_exc);
    }

    #[test]
    fn custom_fingerprint_takes_priority_over_exception() {
        let payload = br#"{"fingerprint":["custom"],"exception":{"values":[{"type":"TypeError","value":"bad"}]}}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();

        // Custom fingerprint trumps everything
        let payload_custom_only = br#"{"fingerprint":["custom"]}"#;
        let fp_custom = compute_fingerprint(1, &ItemType::Event, payload_custom_only).unwrap();
        assert_eq!(fp, fp_custom);
    }

    #[test]
    fn empty_fingerprint_array_falls_through() {
        let payload = br#"{"fingerprint":[],"message":"hello"}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();

        // Empty array — should fall through to message
        let payload_msg = br#"{"message":"hello"}"#;
        let fp_msg = compute_fingerprint(1, &ItemType::Event, payload_msg).unwrap();
        assert_eq!(fp, fp_msg);
    }
}
