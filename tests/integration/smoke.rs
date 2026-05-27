//! Smallest possible test that exercises the harness end-to-end.

use crate::common;

#[tokio::test]
async fn admin_health_ok() {
    let body = reqwest::get(format!("{}/health", common::admin_url()))
        .await
        .expect("GET /health")
        .text()
        .await
        .expect("body");
    assert_eq!(body, "ok");
}

#[tokio::test]
async fn ingest_health_ok() {
    let body = reqwest::get(format!("{}/health", common::ingest_url()))
        .await
        .expect("GET ingest /health")
        .text()
        .await
        .expect("body");
    assert_eq!(body, "ok");
}
