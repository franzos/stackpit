//! Open-mode ingest admission + an ingest→query round-trip. Mirrors the
//! e2e-review skill's tier-2 §2.4 probes plus the round-trip the curl probes
//! can't assert.

use crate::common;

// sentry keys bind globally to one project; fresh per-test ids/keys avoid cross-run cache collisions.

use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn nanos() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

fn fresh_project_id() -> u64 {
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    1_000_000_000 + ((nanos().wrapping_add(seq.wrapping_mul(2_654_435_761))) % 1_000_000_000)
}

/// A never-before-used 32-hex-char sentry key (the DSN public-key format).
fn fresh_key() -> String {
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{:016x}{:016x}",
        nanos(),
        seq.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    )
}

fn envelope(event_id: &str) -> String {
    let payload =
        format!("{{\"event_id\":\"{event_id}\",\"level\":\"error\",\"message\":\"integration round-trip\"}}");
    format!(
        "{{\"event_id\":\"{event_id}\"}}\n{{\"type\":\"event\",\"length\":{}}}\n{}",
        payload.len(),
        payload
    )
}

async fn post_envelope(project_id: u64, sentry_key: Option<&str>, body: String) -> u16 {
    let c = common::client();
    let mut req = c
        .post(format!(
            "{}/api/{project_id}/envelope",
            common::ingest_url()
        ))
        .header("content-type", "application/x-sentry-envelope")
        .body(body);
    if let Some(key) = sentry_key {
        req = req.header("x-sentry-auth", format!("Sentry sentry_key={key}"));
    }
    req.send().await.expect("post envelope").status().as_u16()
}

#[tokio::test]
async fn open_mode_admission_sequence() {
    let pid = fresh_project_id();
    let key_a = fresh_key();
    let key_b = fresh_key();

    // 1. Wrong key on an EXISTING project (project 1 from seed) -> 401.
    let wrong_existing = post_envelope(
        1,
        Some(&fresh_key()),
        envelope("a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1"),
    )
    .await;
    assert_eq!(wrong_existing, 401, "wrong key on existing project -> 401");

    // 2. First event on a brand-new project -> 200 (auto-provisions key_a).
    let first_new = post_envelope(
        pid,
        Some(&key_a),
        envelope("b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2"),
    )
    .await;
    assert_eq!(first_new, 200, "first event on new project -> 200");

    // 3. Different key on the now-existing project -> 401 (first DSN wins).
    let diff_key = post_envelope(
        pid,
        Some(&key_b),
        envelope("c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3"),
    )
    .await;
    assert_eq!(diff_key, 401, "second key on existing project -> 401");

    // 4. No auth at all -> 401.
    let no_auth = post_envelope(pid, None, envelope("d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4")).await;
    assert_eq!(no_auth, 401, "no sentry key -> 401");
}

#[tokio::test]
async fn ingest_then_query_back() {
    let pid = fresh_project_id();
    let key = fresh_key();
    // Unique id so the DB poll and API lookup can't match a row from an earlier run.
    let event_id = format!("{:032x}", nanos());

    let status = post_envelope(pid, Some(&key), envelope(&event_id)).await;
    assert_eq!(status, 200, "ingest should accept the event");

    // The writer batches asynchronously — poll the DB until the row lands.
    let mut landed = false;
    for _ in 0..20 {
        let count = common::db_query(&format!(
            "SELECT COUNT(*) FROM events WHERE event_id = '{event_id}'"
        ));
        if count == "1" {
            landed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    assert!(landed, "event {event_id} never landed in the events table");

    // And it is reachable through the JSON API on the admin port.
    let c = common::login().await;
    let resp = c
        .get(format!("{}/api/v1/events/{event_id}/", common::admin_url()))
        .header("authorization", format!("Bearer {}", common::admin_token()))
        .header("accept", "application/json")
        .send()
        .await
        .expect("GET event by id");
    assert_eq!(
        resp.status().as_u16(),
        200,
        "event should be queryable via API"
    );
}
