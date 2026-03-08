use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use serde_json::json;

/// The happy-path response Sentry SDKs expect -- just the event ID back.
pub fn sentry_response(event_id: &str) -> impl IntoResponse {
    let body = json!({ "id": event_id });
    (StatusCode::OK, axum::Json(body))
}

/// 429 with Retry-After so the SDK knows when to back off.
pub fn rate_limited_response_with_retry(retry_after: u32) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    if let Ok(val) = format!("{retry_after}:error:org").parse() {
        headers.insert("X-Sentry-Rate-Limits", val);
    }
    headers.insert("Retry-After", HeaderValue::from(retry_after));
    (StatusCode::TOO_MANY_REQUESTS, headers, "rate limited")
}

/// 503 when the writer queue is full -- this is backpressure, not rate limiting.
pub fn overloaded_response() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Retry-After", HeaderValue::from_static("60"));
    (
        StatusCode::SERVICE_UNAVAILABLE,
        headers,
        "server overloaded",
    )
}

/// Error response that sets X-Sentry-Error so SDKs can log something useful.
pub fn error_response(status: StatusCode, message: &str) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    if let Ok(val) = message.parse::<HeaderValue>() {
        headers.insert("X-Sentry-Error", val);
    }
    (status, headers, message.to_string())
}
