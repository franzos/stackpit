//! Fingerprinting for issue grouping (priority: SDK > exception > message > transaction > UUID).

use crate::ingest::models::ItemType;
use serde_json::Value;
use std::borrow::Cow;

/// FNV-1a 64-bit: fast, deterministic, sufficient for fingerprinting.
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

// U+0001 so a literal "<uuid>" in attacker-supplied text can't forge a group; 0x00 is the field separator.
const UUID_PLACEHOLDER: &str = "\u{1}uuid\u{1}";
const HEX_PLACEHOLDER: &str = "\u{1}hex\u{1}";
const NUM_PLACEHOLDER: &str = "\u{1}num\u{1}";

/// Identifier char: a run touching one of these is part of a name, not a value.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn run_len(bytes: &[u8], from: usize, pred: fn(&u8) -> bool) -> usize {
    bytes[from..].iter().take_while(|b| pred(b)).count()
}

fn is_uuid_at(bytes: &[u8], from: usize) -> bool {
    let mut at = from;
    for (i, group) in [8usize, 4, 4, 4, 12].into_iter().enumerate() {
        if i > 0 {
            if bytes.get(at) != Some(&b'-') {
                return false;
            }
            at += 1;
        }
        if run_len(bytes, at, u8::is_ascii_hexdigit) != group {
            return false;
        }
        at += group;
    }
    true
}

fn normalize_for_grouping(input: &str) -> Cow<'_, str> {
    let bytes = input.as_bytes();
    let mut out: Option<String> = None;
    let mut copied = 0;
    let mut at = 0;

    while at < bytes.len() {
        // Only measured where a run can start, so every byte is looked at a bounded number of times.
        let after_ident = at > 0 && is_ident_byte(bytes[at - 1]);
        let hex_len = if after_ident {
            0
        } else {
            run_len(bytes, at, u8::is_ascii_hexdigit)
        };
        // A bare number is usually meaningful (status code, signal, line); only `key=NNN` is an interpolated value.
        let digits = if at > 0 && bytes[at - 1] == b'=' {
            run_len(bytes, at, u8::is_ascii_digit)
        } else {
            0
        };

        // Order matters: a UUID contains hex, which contains digits.
        let hit = if !after_ident && is_uuid_at(bytes, at) {
            Some((36, UUID_PLACEHOLDER))
        } else if hex_len >= 16 {
            Some((hex_len, HEX_PLACEHOLDER))
        } else if digits > 0 {
            Some((digits, NUM_PLACEHOLDER))
        } else {
            None
        };

        match hit {
            Some((len, placeholder)) => {
                let buf = out.get_or_insert_with(|| String::with_capacity(input.len()));
                buf.push_str(&input[copied..at]);
                buf.push_str(placeholder);
                at += len;
                copied = at;
            }
            None => at += 1,
        }
    }

    match out {
        Some(mut buf) => {
            buf.push_str(&input[copied..]);
            Cow::Owned(buf)
        }
        None => Cow::Borrowed(input),
    }
}

/// Fingerprint from an already-parsed JSON value.
/// Returns `None` for item types that don't produce issues (sessions, client reports, etc.).
pub fn compute_fingerprint_from_value(
    project_id: u64,
    item_type: &ItemType,
    json: &Value,
) -> Option<String> {
    if !item_type.can_fingerprint() {
        return None;
    }

    compute_fingerprint_inner(project_id, json)
}

/// Fingerprint from raw JSON bytes.
/// Returns `None` for non-issue item types. Falls back to a random UUID
/// on unparseable JSON (better to store an ungrouped event than drop it).
pub fn compute_fingerprint(
    project_id: u64,
    item_type: &ItemType,
    payload_json: &[u8],
) -> Option<String> {
    if !item_type.can_fingerprint() {
        return None;
    }

    let json: Value = match serde_json::from_slice(payload_json) {
        Ok(v) => v,
        Err(_) => return Some(uuid::Uuid::new_v4().to_string()),
    };

    compute_fingerprint_inner(project_id, &json)
}

// The hash is a grouping key, not a security boundary: a constructed collision
// is bounded by the `(project_id, fingerprint)` key every issue table uses.
fn compute_fingerprint_inner(project_id: u64, json: &Value) -> Option<String> {
    // SDK-provided fingerprint array wins, unless it's just ["{{ default }}"].
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
                    input.extend_from_slice(elem.to_string().as_bytes());
                }
            }
            return Some(format_hash(fnv1a_64(&input)));
        }
    }

    // Null bytes between project/type/value prevent cross-field collisions.
    if let Some(exc) = json
        .get("exception")
        .and_then(|e| e.get("values"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
    {
        let exc_type = exc.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let exc_value = crate::ingest::lenient::lenient_field(exc, "value").unwrap_or_default();

        let mut input = Vec::new();
        input.extend_from_slice(project_id.to_string().as_bytes());
        input.push(0x00);
        input.extend_from_slice(exc_type.as_bytes());
        input.push(0x00);
        input.extend_from_slice(normalize_for_grouping(&exc_value).as_bytes());
        return Some(format_hash(fnv1a_64(&input)));
    }

    // logentry.message is the unformatted template (what we group on); top-level `message` is the fallback.
    if let Some(msg) = crate::ingest::lenient::message_text(json) {
        let mut input = Vec::new();
        input.extend_from_slice(project_id.to_string().as_bytes());
        input.push(0x00);
        input.extend_from_slice(normalize_for_grouping(&msg).as_bytes());
        return Some(format_hash(fnv1a_64(&input)));
    }

    // Transaction name: last structured option before falling back.
    if let Some(txn) = json.get("transaction").and_then(|v| v.as_str()) {
        let mut input = Vec::new();
        input.extend_from_slice(project_id.to_string().as_bytes());
        input.push(0x00);
        input.extend_from_slice(txn.as_bytes());
        return Some(format_hash(fnv1a_64(&input)));
    }

    // Nothing to group on: random UUID, each event becomes its own issue.
    Some(uuid::Uuid::new_v4().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_known_value() {
        // Empty input returns the offset basis (the FNV starting point).
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
        assert!(compute_fingerprint(1, &ItemType::Transaction, payload).is_none());
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
    fn only_event_returns_some() {
        let payload = br#"{"message":"hello"}"#;
        assert!(compute_fingerprint(1, &ItemType::Event, payload).is_some());
        // Transactions no longer produce issues; they feed transaction_metrics.
        assert!(compute_fingerprint(1, &ItemType::Transaction, payload).is_none());
    }

    #[test]
    fn custom_fingerprint_array() {
        let payload = br#"{"fingerprint":["my-custom-group","extra"],"message":"hello"}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();
        assert_eq!(fp.len(), 16);

        // Same custom fingerprint, different project: must not collide.
        let payload2 = br#"{"fingerprint":["my-custom-group","extra"],"message":"different"}"#;
        let fp2 = compute_fingerprint(999, &ItemType::Event, payload2).unwrap();
        assert_ne!(fp, fp2);

        // Same custom fingerprint, same project: deterministic.
        let fp3 = compute_fingerprint(1, &ItemType::Event, payload).unwrap();
        assert_eq!(fp, fp3);
    }

    #[test]
    fn default_fingerprint_falls_through() {
        // ["{{ default }}"] means "use normal grouping": it must not override.
        let payload = br#"{"fingerprint":["{{ default }}"],"message":"hello"}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();

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

        // Same exception, same project: deterministic.
        let fp2 = compute_fingerprint(42, &ItemType::Event, payload).unwrap();
        assert_eq!(fp, fp2);

        // Different project: must not collide.
        let fp3 = compute_fingerprint(43, &ItemType::Event, payload).unwrap();
        assert_ne!(fp, fp3);
    }

    // An array-valued `value` used to read as absent, collapsing every React
    // Native console error in a project into one type-only issue.
    #[test]
    fn array_valued_exception_groups_by_message() {
        let a = br#"{"exception":{"values":[{"type":"Error","value":["consent unknown"]}]}}"#;
        let b = br#"{"exception":{"values":[{"type":"Error","value":["token expired"]}]}}"#;
        let fp_a = compute_fingerprint(1, &ItemType::Event, a).unwrap();
        let fp_b = compute_fingerprint(1, &ItemType::Event, b).unwrap();
        assert_ne!(fp_a, fp_b, "distinct messages must be distinct issues");

        // Coerced identically to the equivalent plain-string payload.
        let plain = br#"{"exception":{"values":[{"type":"Error","value":"consent unknown"}]}}"#;
        assert_eq!(
            fp_a,
            compute_fingerprint(1, &ItemType::Event, plain).unwrap()
        );
    }

    #[test]
    fn chained_exceptions_uses_first() {
        let payload = br#"{"exception":{"values":[
            {"type":"ValueError","value":"bad value"},
            {"type":"RuntimeError","value":"runtime issue"}
        ]}}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();

        // Only the first exception in the chain drives grouping.
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

        // Same message, same project: deterministic.
        let fp2 = compute_fingerprint(1, &ItemType::Event, payload).unwrap();
        assert_eq!(fp, fp2);

        // Different message: different fingerprint.
        let payload2 = br#"{"message":"something else broke"}"#;
        let fp3 = compute_fingerprint(1, &ItemType::Event, payload2).unwrap();
        assert_ne!(fp, fp3);
    }

    #[test]
    fn logentry_template_preferred_over_formatted() {
        // logentry.message is the unformatted template, which is what we group on.
        let payload =
            br#"{"logentry":{"message":"User %s logged in","formatted":"User alice logged in"}}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();

        // Same template, different rendered value: must still group together.
        let payload2 =
            br#"{"logentry":{"message":"User %s logged in","formatted":"User bob logged in"}}"#;
        let fp2 = compute_fingerprint(1, &ItemType::Event, payload2).unwrap();
        assert_eq!(fp, fp2);
    }

    #[test]
    fn logentry_preferred_over_top_level_message() {
        let payload = br#"{"logentry":{"message":"template %s"},"message":"rendered value"}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();

        // logentry.message takes priority over top-level message.
        let payload_logentry_only = br#"{"logentry":{"message":"template %s"}}"#;
        let fp2 = compute_fingerprint(1, &ItemType::Event, payload_logentry_only).unwrap();
        assert_eq!(fp, fp2);
    }

    #[test]
    fn transaction_name_fingerprint_for_event() {
        // Error events with a `transaction` field but no exception/message group by transaction name.
        let payload = br#"{"transaction":"/api/health"}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();
        assert_eq!(fp.len(), 16);
    }

    #[test]
    fn fallback_uuid_for_empty_event() {
        let payload = br#"{"level":"info"}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();
        // UUID fallback: 36 chars with dashes.
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
        // Null separator prevents "TypeError" + "" colliding with "Type" + "Error".
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

        // Exception wins over message in the priority chain.
        let payload_exc_only = br#"{"exception":{"values":[{"type":"TypeError","value":"bad"}]}}"#;
        let fp_exc = compute_fingerprint(1, &ItemType::Event, payload_exc_only).unwrap();
        assert_eq!(fp, fp_exc);
    }

    #[test]
    fn custom_fingerprint_takes_priority_over_exception() {
        let payload = br#"{"fingerprint":["custom"],"exception":{"values":[{"type":"TypeError","value":"bad"}]}}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();

        // Custom fingerprint trumps everything.
        let payload_custom_only = br#"{"fingerprint":["custom"]}"#;
        let fp_custom = compute_fingerprint(1, &ItemType::Event, payload_custom_only).unwrap();
        assert_eq!(fp, fp_custom);
    }

    #[test]
    fn normalize_leaves_untouched_text_borrowed() {
        assert!(matches!(
            normalize_for_grouping("plain failure, no values"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn normalize_replaces_values_in_order() {
        assert_eq!(
            normalize_for_grouping("user c202a250-f4a1-4820-9d30-0178cb91d67f gone"),
            format!("user {UUID_PLACEHOLDER} gone")
        );
        assert_eq!(
            normalize_for_grouping("token deadbeefcafebabe1234 rejected"),
            format!("token {HEX_PLACEHOLDER} rejected")
        );
        assert_eq!(
            normalize_for_grouping("done_in=136 ms"),
            format!("done_in={NUM_PLACEHOLDER} ms")
        );
    }

    #[test]
    fn normalize_keeps_digits_that_belong_to_names() {
        // utf8 / v2 / sha256 are names, not interpolated values.
        assert_eq!(
            normalize_for_grouping("utf8 decode failed on /api/v2 via sha256"),
            "utf8 decode failed on /api/v2 via sha256"
        );
    }

    #[test]
    fn num_only_fires_on_key_value_form() {
        assert_eq!(
            normalize_for_grouping("done_in=136 ms"),
            format!("done_in={NUM_PLACEHOLDER} ms")
        );
        // In `key=value` a unit suffix belongs to the value.
        assert_eq!(
            normalize_for_grouping("done_in=136ms"),
            format!("done_in={NUM_PLACEHOLDER}ms")
        );
        assert_eq!(
            normalize_for_grouping("attempt=3"),
            format!("attempt={NUM_PLACEHOLDER}")
        );

        for unchanged in [
            "HTTP 500 from upstream",
            "HTTP 502 from upstream",
            "killed by signal 9",
            "exit code 1",
            "code 137",
            "panic at src/main.rs:42:9",
            "SQLSTATE 23505",
            "café123",
        ] {
            assert_eq!(normalize_for_grouping(unchanged), unchanged);
        }
    }

    #[test]
    fn status_codes_stay_separate() {
        let a = br#"{"message":"HTTP 500 from upstream"}"#;
        let b = br#"{"message":"HTTP 502 from upstream"}"#;
        assert_ne!(
            compute_fingerprint(1, &ItemType::Event, a).unwrap(),
            compute_fingerprint(1, &ItemType::Event, b).unwrap()
        );
    }

    #[test]
    fn hex_needs_a_word_boundary() {
        assert_eq!(
            normalize_for_grouping("orderid123456789012345"),
            "orderid123456789012345"
        );
        assert_eq!(
            normalize_for_grouping("z0123456789abcdef done"),
            "z0123456789abcdef done"
        );
        assert_eq!(
            normalize_for_grouping("0123456789abcdef done"),
            format!("{HEX_PLACEHOLDER} done")
        );
    }

    #[test]
    fn hex_threshold_is_sixteen() {
        assert_eq!(normalize_for_grouping("0123456789abcde"), "0123456789abcde");
        assert_eq!(normalize_for_grouping("0123456789abcdef"), HEX_PLACEHOLDER);
        assert_eq!(normalize_for_grouping("0123456789abcdef0"), HEX_PLACEHOLDER);
    }

    #[test]
    fn normalize_handles_multibyte_input() {
        assert_eq!(normalize_for_grouping("café123"), "café123");
        assert_eq!(
            normalize_for_grouping("naïve retry done_in=42 ms"),
            format!("naïve retry done_in={NUM_PLACEHOLDER} ms")
        );
        assert_eq!(
            normalize_for_grouping("日本語 c202a250-f4a1-4820-9d30-0178cb91d67f 失敗"),
            format!("日本語 {UUID_PLACEHOLDER} 失敗")
        );
    }

    #[test]
    fn normalize_handles_uuid_edges() {
        assert_eq!(
            normalize_for_grouping("C202A250-F4A1-4820-9D30-0178CB91D67F"),
            UUID_PLACEHOLDER
        );
        assert_eq!(
            normalize_for_grouping("c202a250-f4a1-4820-9d30-0178cb91d67f failed"),
            format!("{UUID_PLACEHOLDER} failed")
        );
        assert_eq!(
            normalize_for_grouping("failed for c202a250-f4a1-4820-9d30-0178cb91d67f"),
            format!("failed for {UUID_PLACEHOLDER}")
        );
        assert_eq!(normalize_for_grouping(""), "");
    }

    #[test]
    fn literal_placeholder_text_cannot_forge_a_group() {
        assert_eq!(
            normalize_for_grouping("user <uuid> gone"),
            "user <uuid> gone"
        );

        let literal = br#"{"message":"user <uuid> gone"}"#;
        let real = br#"{"message":"user c202a250-f4a1-4820-9d30-0178cb91d67f gone"}"#;
        assert_ne!(
            compute_fingerprint(1, &ItemType::Event, literal).unwrap(),
            compute_fingerprint(1, &ItemType::Event, real).unwrap()
        );
    }

    #[test]
    fn interpolated_uuid_groups_together() {
        let a = br#"{"message":"Failed to send push notification for user c202a250-f4a1-4820-9d30-0178cb91d67f"}"#;
        let b = br#"{"message":"Failed to send push notification for user dd1ffcfa-765c-42bb-a576-ed3c1ea8177f"}"#;
        assert_eq!(
            compute_fingerprint(1, &ItemType::Event, a).unwrap(),
            compute_fingerprint(1, &ItemType::Event, b).unwrap()
        );
    }

    #[test]
    fn interpolated_duration_groups_together() {
        let a = br#"{"message":"Job Failed: (status 401 Unauthorized): Token is not active done_in=136 ms"}"#;
        let b = br#"{"message":"Job Failed: (status 401 Unauthorized): Token is not active done_in=245 ms"}"#;
        assert_eq!(
            compute_fingerprint(1, &ItemType::Event, a).unwrap(),
            compute_fingerprint(1, &ItemType::Event, b).unwrap()
        );
    }

    #[test]
    fn normalization_still_separates_different_status_codes() {
        let a = br#"{"message":"Job Failed: (status 401 Unauthorized) done_in=136 ms"}"#;
        let b = br#"{"message":"Job Failed: (status 403 Unauthorized) done_in=136 ms"}"#;
        let c = br#"{"message":"Job Failed: (status 401 Unauthorized) done_in=245 ms"}"#;
        assert_ne!(
            compute_fingerprint(1, &ItemType::Event, a).unwrap(),
            compute_fingerprint(1, &ItemType::Event, b).unwrap()
        );
        assert_eq!(
            compute_fingerprint(1, &ItemType::Event, a).unwrap(),
            compute_fingerprint(1, &ItemType::Event, c).unwrap()
        );
    }

    #[test]
    fn exception_value_normalizes_but_type_still_separates() {
        let a = br#"{"exception":{"values":[{"type":"PushError","value":"no device for c202a250-f4a1-4820-9d30-0178cb91d67f"}]}}"#;
        let b = br#"{"exception":{"values":[{"type":"PushError","value":"no device for dd1ffcfa-765c-42bb-a576-ed3c1ea8177f"}]}}"#;
        assert_eq!(
            compute_fingerprint(1, &ItemType::Event, a).unwrap(),
            compute_fingerprint(1, &ItemType::Event, b).unwrap()
        );

        let c = br#"{"exception":{"values":[{"type":"SendError","value":"no device for c202a250-f4a1-4820-9d30-0178cb91d67f"}]}}"#;
        assert_ne!(
            compute_fingerprint(1, &ItemType::Event, a).unwrap(),
            compute_fingerprint(1, &ItemType::Event, c).unwrap()
        );
    }

    #[test]
    fn client_fingerprint_array_is_not_normalized() {
        // An SDK grouping key is deliberate: per-id groups stay per-id.
        let a = br#"{"fingerprint":["push","c202a250-f4a1-4820-9d30-0178cb91d67f"]}"#;
        let b = br#"{"fingerprint":["push","dd1ffcfa-765c-42bb-a576-ed3c1ea8177f"]}"#;
        assert_ne!(
            compute_fingerprint(1, &ItemType::Event, a).unwrap(),
            compute_fingerprint(1, &ItemType::Event, b).unwrap()
        );
    }

    #[test]
    fn transaction_name_is_not_normalized() {
        let a = br#"{"transaction":"/api/users/1234"}"#;
        let b = br#"{"transaction":"/api/users/5678"}"#;
        assert_ne!(
            compute_fingerprint(1, &ItemType::Event, a).unwrap(),
            compute_fingerprint(1, &ItemType::Event, b).unwrap()
        );
    }

    #[test]
    fn empty_fingerprint_array_falls_through() {
        let payload = br#"{"fingerprint":[],"message":"hello"}"#;
        let fp = compute_fingerprint(1, &ItemType::Event, payload).unwrap();

        // Empty array falls through to message.
        let payload_msg = br#"{"message":"hello"}"#;
        let fp_msg = compute_fingerprint(1, &ItemType::Event, payload_msg).unwrap();
        assert_eq!(fp, fp_msg);
    }
}
