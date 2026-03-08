use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;

use crate::auth::SentryAuth;
use crate::filter::{FilterEngine, FilterVerdict, PreFilterReject};
use crate::models::StorableEvent;
use crate::network;
use crate::server::AppState;

use super::responses::{rate_limited_response_with_retry, sentry_response};

#[allow(clippy::result_large_err)]
pub async fn authenticate_and_prefilter(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
    project_id: u64,
    addr: std::net::SocketAddr,
) -> Result<SentryAuth, axum::response::Response> {
    let auth = super::auth::authenticate(state, headers, uri, project_id).await?;
    pre_filter(
        &state.filter_engine,
        headers,
        &auth.sentry_key,
        project_id,
        Some(addr),
    )?;
    Ok(auth)
}

/// Runs the event through the filter engine. Returns `true` when the event
/// gets dropped -- and records why in discard_stats so we can surface it later.
pub fn check_event_filter(state: &AppState, event: &StorableEvent, project_id: u64) -> bool {
    if let FilterVerdict::Drop { reason } = state.filter_engine.check(event) {
        tracing::debug!("filtered event {}: {reason}", event.event_id);
        state
            .discard_stats
            .record(project_id, reason.as_str(), None);
        return true;
    }
    false
}

/// Lightweight checks we can do before parsing the body -- rate limits,
/// user-agent blocks, IP blocks. Grabs the filter snapshot once and runs
/// everything against it. Rejects early if anything trips.
#[allow(clippy::result_large_err)]
pub fn pre_filter(
    filter_engine: &FilterEngine,
    headers: &HeaderMap,
    sentry_key: &str,
    project_id: u64,
    connect_addr: Option<std::net::SocketAddr>,
) -> Result<(), axum::response::Response> {
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok());
    let client_ip = network::extract_client_ip(headers, connect_addr);

    match filter_engine.pre_filter_check(sentry_key, project_id, ua, client_ip.as_deref()) {
        Ok(()) => Ok(()),
        Err(PreFilterReject::RateLimited(retry_after)) => {
            Err(rate_limited_response_with_retry(retry_after).into_response())
        }
        Err(PreFilterReject::DroppedUserAgent | PreFilterReject::DroppedIp) => {
            let placeholder = uuid::Uuid::new_v4().to_string();
            Err(sentry_response(&placeholder).into_response())
        }
    }
}
