#![no_main]

use libfuzzer_sys::fuzz_target;
use stackpit::fuzz_api::{
    extract_embedded_spans_from_value, extract_span_fields, log_entries_from_value,
    parse_log_entries, parse_log_entry, parse_metric_payload,
};

fuzz_target!(|body: &[u8]| {
    for entry in parse_log_entries(body) {
        let _ = parse_log_entry(&entry);
    }
    let _ = parse_metric_payload(body);
    let _ = extract_span_fields(body);
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) {
        let _ = extract_embedded_spans_from_value(&json);
        let _ = log_entries_from_value(json);
    }
});
