use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};

use crate::commercial::license::{Feature, FeatureStatus};

const LATENCY_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

pub fn install_metrics_recorder() -> PrometheusHandle {
    let recorder = PrometheusBuilder::new()
        .set_buckets_for_metric(Matcher::Suffix("_seconds".to_string()), LATENCY_BUCKETS)
        .expect("valid histogram buckets")
        .build_recorder();
    let handle = recorder.handle();
    if metrics::set_global_recorder(recorder).is_err() {
        tracing::warn!("metrics recorder already installed; /metrics may render stale data");
    }
    handle
}

pub fn scrape_allowed(status: FeatureStatus) -> bool {
    matches!(status, FeatureStatus::Allowed | FeatureStatus::GraceReadOnly)
}

pub fn record_bridged_metrics(accepted: u64, rejected: u64, dropped: u64) {
    metrics::counter!("stackpit_events_accepted_total").absolute(accepted);
    metrics::counter!("stackpit_events_rejected_total").absolute(rejected);
    metrics::counter!("stackpit_events_dropped_total").absolute(dropped);
}

use std::sync::atomic::Ordering;
use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::server::AppState;

pub async fn track_http_metrics(req: Request, next: Next) -> Response {
    let start = Instant::now();
    // Bounded allowlist: a raw method string is attacker-controllable and unbounded (ingest listener is public).
    let method = match *req.method() {
        Method::GET => "GET",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::DELETE => "DELETE",
        Method::PATCH => "PATCH",
        Method::HEAD => "HEAD",
        Method::OPTIONS => "OPTIONS",
        Method::TRACE => "TRACE",
        Method::CONNECT => "CONNECT",
        _ => "OTHER",
    }
    .to_owned();
    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".to_owned());

    let response = next.run(req).await;

    let status = response.status().as_u16().to_string();
    let labels = [("method", method), ("path", path), ("status", status)];
    metrics::counter!("http_requests_total", &labels).increment(1);
    metrics::histogram!("http_request_duration_seconds", &labels)
        .record(start.elapsed().as_secs_f64());
    response
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn bearer_matches(headers: &HeaderMap, expected: &str) -> bool {
    let Some(value) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return false;
    };
    ct_eq(token.as_bytes(), expected.as_bytes())
}

pub async fn metrics_handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !scrape_allowed(state.license.feature(Feature::Observability)) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(expected) = state.metrics_scrape_token.as_deref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !bearer_matches(&headers, expected) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let s = &state.ingest_stats;
    record_bridged_metrics(
        s.events_accepted.load(Ordering::Relaxed),
        s.events_rejected.load(Ordering::Relaxed),
        s.events_dropped.load(Ordering::Relaxed),
    );
    let body = state.metrics_handle.render();
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commercial::license::FeatureStatus;
    use metrics_exporter_prometheus::PrometheusBuilder;

    #[test]
    fn scrape_allowed_matrix() {
        assert!(scrape_allowed(FeatureStatus::Allowed));
        assert!(scrape_allowed(FeatureStatus::GraceReadOnly));
        assert!(!scrape_allowed(FeatureStatus::Locked));
    }

    #[test]
    fn bridged_metrics_render() {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        metrics::with_local_recorder(&recorder, || {
            record_bridged_metrics(10, 2, 1);
        });
        let out = handle.render();
        assert!(out.contains("stackpit_events_accepted_total 10"), "got:\n{out}");
        assert!(out.contains("stackpit_events_rejected_total 2"), "got:\n{out}");
        assert!(out.contains("stackpit_events_dropped_total 1"), "got:\n{out}");
    }

    #[tokio::test]
    async fn http_metrics_recorded() {
        use axum::{routing::get, Router};
        use tower::ServiceExt;

        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        let app = Router::new()
            .route("/ping", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(super::track_http_metrics));

        metrics::with_local_recorder(&recorder, || {
            futures::executor::block_on(async {
                let _ = app
                    .clone()
                    .oneshot(axum::http::Request::builder().uri("/ping").body(axum::body::Body::empty()).unwrap())
                    .await
                    .unwrap();
            });
        });

        let out = handle.render();
        assert!(out.contains("http_requests_total"), "got:\n{out}");
        assert!(out.contains("path=\"/ping\""), "got:\n{out}");
    }

    #[test]
    fn bearer_matches_matrix() {
        use axum::http::{header, HeaderMap, HeaderValue};
        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        assert!(super::bearer_matches(&h, "secret"));
        assert!(!super::bearer_matches(&h, "wrong"));
        assert!(!super::bearer_matches(&HeaderMap::new(), "secret"));
        let mut basic = HeaderMap::new();
        basic.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic secret"));
        assert!(!super::bearer_matches(&basic, "secret"));
    }
}
