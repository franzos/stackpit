#![no_main]

use libfuzzer_sys::fuzz_target;
use stackpit::ingest::event_data::{
    extract_breadcrumbs, extract_contexts, extract_exceptions, extract_extra, extract_measurements,
    extract_request, extract_summary_tags, extract_tags, extract_trace_id, extract_user,
};
use stackpit::ingest::fingerprint::compute_fingerprint_from_value;
use stackpit::ingest::models::ItemType;
use stackpit::ingest::replay_meta;

const PROJECT_ID: u64 = 7;

fuzz_target!(|body: &[u8]| {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) else {
        return;
    };
    let _ = extract_exceptions(&json, None, &[], None);
    let _ = extract_exceptions(&json, Some("deadbeef"), &[], None);
    let _ = extract_breadcrumbs(&json);
    let tags = extract_tags(&json);
    let contexts = extract_contexts(&json);
    let _ = extract_summary_tags(&tags, &contexts);
    let _ = extract_request(&json);
    let _ = extract_user(&json);
    let _ = extract_extra(&json);
    let _ = extract_trace_id(&json);
    let _ = extract_measurements(&json);
    let _ = replay_meta::extract(&json, 0);

    // Grouping is a pure function of the payload, except for events with
    // nothing to group on: those get a random UUID so each is its own issue.
    let first = compute_fingerprint_from_value(PROJECT_ID, &ItemType::Event, &json)
        .expect("events always fingerprint");
    let second = compute_fingerprint_from_value(PROJECT_ID, &ItemType::Event, &json)
        .expect("events always fingerprint");
    if first.len() == 16 {
        assert_eq!(first, second);
    } else {
        assert_eq!(first.len(), 36);
        assert_eq!(second.len(), 36);
    }
    let _ = compute_fingerprint_from_value(PROJECT_ID, &ItemType::Transaction, &json);
});
