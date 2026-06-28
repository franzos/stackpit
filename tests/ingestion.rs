use stackpit::db;
use stackpit::ingest::models::{ItemType, Level, StorableEvent};
use stackpit::util::stats::IngestStats;
use stackpit::writer::{self, WriteMsg};

fn make_event(event_id: &str, project_id: u64, fingerprint: &str) -> StorableEvent {
    let payload = serde_json::json!({
        "event_id": event_id,
        "message": format!("test error {event_id}"),
        "level": "error",
        "timestamp": 1700000000.0,
    });
    let raw = serde_json::to_vec(&payload).unwrap();

    StorableEvent {
        event_id: event_id.to_string(),
        item_type: ItemType::Event,
        payload: raw,
        project_id,
        public_key: "test-key".to_string(),
        timestamp: 1700000000,
        level: Some(Level::Error),
        platform: Some("python".to_string()),
        release: Some("1.0.0".to_string()),
        environment: Some("production".to_string()),
        server_name: None,
        transaction_name: None,
        title: Some(format!("test error {event_id}")),
        sdk_name: None,
        sdk_version: None,
        fingerprint: Some(fingerprint.to_string()),
        monitor_slug: None,
        session_status: None,
        parent_event_id: None,
        user_identifier: Some("user-1".to_string()),
        tags: vec![("browser".to_string(), "Chrome".to_string())],
        session_buckets: Vec::new(),
        trace_id: None,
        duration_ms: None,
        trace_status: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ingest_event_then_query_back() {
    let pool = db::open_test_pool().await;

    // Queries need an existing project row.
    sqlx::query("INSERT INTO projects (project_id, name) VALUES (1, 'test-project')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO project_keys (public_key, project_id, status) VALUES ('test-key', 1, 'active')")
        .execute(&pool).await.unwrap();

    let (writer, _join) =
        writer::spawn(pool.clone(), None, std::sync::Arc::new(IngestStats::new()))
            .await
            .unwrap();
    let tx = writer.raw_sender();

    let event1 = make_event("evt-001", 1, "fp-001");
    let event2 = make_event("evt-002", 1, "fp-001"); // same fingerprint, should land in same issue

    tx.try_send(WriteMsg::Event(event1)).unwrap();
    tx.try_send(WriteMsg::Event(event2)).unwrap();

    // Shut down cleanly so everything flushes.
    let _ = writer.shutdown();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let detail = stackpit::queries::events::get_event_detail(&pool, "evt-001")
        .await
        .unwrap();
    assert!(
        detail.is_some(),
        "event should be queryable after ingestion"
    );
    let detail = detail.unwrap();
    assert_eq!(detail.event_id, "evt-001");

    let issue = stackpit::queries::issues::get_issue(&pool, "fp-001")
        .await
        .unwrap();
    assert!(issue.is_some(), "issue should exist for the fingerprint");
    let issue = issue.unwrap();
    assert_eq!(issue.fingerprint, "fp-001");
    assert!(
        issue.event_count >= 2,
        "issue should have at least 2 events"
    );

    let page = stackpit::queries::types::Page::new(None, None);
    let events = stackpit::queries::events::list_events(&pool, 1, &page)
        .await
        .unwrap();
    assert!(
        events.items.len() >= 2,
        "project should have at least 2 events"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ingest_and_update_issue_status() {
    let pool = db::open_test_pool().await;

    sqlx::query("INSERT INTO projects (project_id, name) VALUES (1, 'test-project')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO project_keys (public_key, project_id, status) VALUES ('test-key', 1, 'active')")
        .execute(&pool).await.unwrap();

    let (writer, _join) =
        writer::spawn(pool.clone(), None, std::sync::Arc::new(IngestStats::new()))
            .await
            .unwrap();

    writer
        .send_event(make_event("evt-status-1", 1, "fp-status-001"))
        .unwrap();

    // Wait past the 1s flush interval, then send another event to trigger
    // the aggregation flush that creates the issue row.
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    writer
        .send_event(make_event("evt-status-2", 1, "fp-status-002"))
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let rows = stackpit::queries::issues::update_issue_status(
        &pool,
        "fp-status-001",
        stackpit::queries::IssueStatus::Resolved,
    )
    .await
    .unwrap();
    assert_eq!(rows, 1, "status update should affect one row");

    let _ = writer.shutdown();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let issue = stackpit::queries::issues::get_issue(&pool, "fp-status-001")
        .await
        .unwrap();
    assert!(issue.is_some());
    let issue = issue.unwrap();
    assert_eq!(issue.status, stackpit::queries::IssueStatus::Resolved);
}
