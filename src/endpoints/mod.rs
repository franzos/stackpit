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
use axum::routing::post;
use axum::Router;
use tower_http::decompression::RequestDecompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;

/// Ingest routes (envelope/store/security/minidump) with the tight body-limit
/// and decompression stack: compressed read -> decompress -> decompressed body.
/// `compressed_body_limit` caps the pre-decompression read; `max_body_size` the
/// post-decompression body. State is left unbound for the caller to wire.
pub(crate) fn routes(max_body_size: usize, compressed_body_limit: usize) -> Router<AppState> {
    Router::new()
        .route("/api/{project_id}/envelope/", post(envelope::handle))
        .route("/api/{project_id}/envelope", post(envelope::handle))
        .route("/api/{project_id}/store/", post(store::handle))
        .route("/api/{project_id}/store", post(store::handle))
        .route("/api/{project_id}/security/", post(security::handle))
        .route("/api/{project_id}/security", post(security::handle))
        .route("/api/{project_id}/minidump/", post(minidump::handle))
        .route("/api/{project_id}/minidump", post(minidump::handle))
        .layer(RequestBodyLimitLayer::new(max_body_size))
        .layer(RequestDecompressionLayer::new())
        .layer(RequestBodyLimitLayer::new(compressed_body_limit))
}
