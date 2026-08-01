pub mod auth;
pub mod envelope;
pub mod minidump;
pub mod pipeline;
pub mod responses;
pub mod security;
pub mod store;

pub use pipeline::{authenticate_and_prefilter, check_event_filter};
pub use responses::{
    error_response, overloaded_response, sentry_response, sentry_response_with_discarded,
};

use crate::server::AppState;
use axum::extract::DefaultBodyLimit;
use axum::routing::post;
use axum::Router;
use tower_http::decompression::RequestDecompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;

/// Ingest routes (envelope/store/security/minidump) with the tight body-limit
/// and decompression stack: compressed read -> decompress -> decompressed body.
/// `compressed_body_limit` caps the pre-decompression read; `max_body_size` the
/// post-decompression body. State is left unbound for the caller to wire.
pub(crate) fn routes(max_body_size: usize, compressed_body_limit: usize) -> Router<AppState> {
    let routes = Router::new()
        .route("/api/{project_id}/envelope/", post(envelope::handle))
        .route("/api/{project_id}/envelope", post(envelope::handle))
        .route("/api/{project_id}/store/", post(store::handle))
        .route("/api/{project_id}/store", post(store::handle))
        .route("/api/{project_id}/security/", post(security::handle))
        .route("/api/{project_id}/security", post(security::handle))
        .route("/api/{project_id}/minidump/", post(minidump::handle))
        .route("/api/{project_id}/minidump", post(minidump::handle));
    body_limits(routes, max_body_size, compressed_body_limit)
}

/// The body-limit/decompression stack, split out so it can be exercised without
/// an `AppState`. `DefaultBodyLimit` must track `max_body_size` or axum's 2MiB
/// extractor default silently caps the operator's configured limit.
fn body_limits<S>(
    router: Router<S>,
    max_body_size: usize,
    compressed_body_limit: usize,
) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        .layer(DefaultBodyLimit::max(max_body_size))
        .layer(RequestBodyLimitLayer::new(max_body_size))
        .layer(RequestDecompressionLayer::new())
        .layer(RequestBodyLimitLayer::new(compressed_body_limit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::Multipart;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn limited_app(max_body_size: usize) -> Router {
        let routes = Router::new()
            .route(
                "/bytes",
                post(|body: axum::body::Bytes| async move { body.len().to_string() }),
            )
            .route(
                "/multipart",
                post(|mut mp: Multipart| async move {
                    let mut seen = 0usize;
                    while let Ok(Some(field)) = mp.next_field().await {
                        seen += field.bytes().await.map(|b| b.len()).unwrap_or(0);
                    }
                    seen.to_string()
                }),
            );
        body_limits(routes, max_body_size, max_body_size * 2)
    }

    async fn post_bytes(path: &str, len: usize, content_type: &str) -> StatusCode {
        // 8MiB configured limit: well above axum's 2MiB extractor default.
        let app = limited_app(8 * 1024 * 1024);
        let body = vec![b'x'; len];
        app.oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", content_type)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
    }

    // Regression: the configured max_body_size must be authoritative for the
    // Bytes extractor, not axum's 2MiB DefaultBodyLimit.
    #[tokio::test]
    async fn body_above_axum_default_but_under_configured_limit_is_accepted() {
        assert_eq!(
            post_bytes("/bytes", 3 * 1024 * 1024, "application/octet-stream").await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn body_above_configured_limit_is_rejected() {
        assert_eq!(
            post_bytes("/bytes", 9 * 1024 * 1024, "application/octet-stream").await,
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    // The minidump handler reads through Multipart, which honors the same limit.
    #[tokio::test]
    async fn multipart_above_axum_default_is_accepted() {
        let boundary = "X-BOUNDARY";
        let head = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"upload_file_minidump\"; filename=\"d.dmp\"\r\n\r\n"
        );
        let tail = format!("\r\n--{boundary}--\r\n");
        let mut body = head.into_bytes();
        body.resize(body.len() + 3 * 1024 * 1024, b'x');
        body.extend_from_slice(tail.as_bytes());

        let app = limited_app(8 * 1024 * 1024);
        let status = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/multipart")
                    .header(
                        "content-type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status();
        assert_eq!(status, StatusCode::OK);
    }
}
