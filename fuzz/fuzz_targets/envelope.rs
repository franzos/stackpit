#![no_main]

use libfuzzer_sys::fuzz_target;
use stackpit::ingest::auth::SentryAuth;
use stackpit::ingest::enrich::enrich_event;
use stackpit::ingest::envelope::{parse, parse_security_body, parse_store_body};

const PROJECT_ID: u64 = 7;

fuzz_target!(|body: &[u8]| {
    let auth = SentryAuth {
        sentry_key: "fuzzkey".to_string(),
    };
    if let Ok(parsed) = parse(body, PROJECT_ID, &auth) {
        for mut event in parsed.events {
            assert_eq!(event.project_id, PROJECT_ID);
            assert!(!event.event_id.is_empty());
            enrich_event(&mut event);
        }
    }
    if let Ok(mut event) = parse_store_body(body, PROJECT_ID, &auth) {
        assert!(!event.event_id.is_empty());
        enrich_event(&mut event);
    }
    if let Ok(mut event) = parse_security_body(body, PROJECT_ID, &auth) {
        assert!(!event.event_id.is_empty());
        enrich_event(&mut event);
    }
});
