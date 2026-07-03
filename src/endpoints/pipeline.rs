use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;

use crate::filter::{FilterEngine, FilterVerdict, PreFilterReject};
use crate::ingest::auth::SentryAuth;
use crate::ingest::models::StorableEvent;
use crate::server::AppState;
use crate::util::network;

use super::responses::{rate_limited_response_with_retry, sentry_response};

#[allow(clippy::result_large_err)]
pub async fn authenticate_and_prefilter(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
    project_id: u64,
    addr: std::net::SocketAddr,
) -> Result<SentryAuth, axum::response::Response> {
    // Per-IP auth-failure cutoff: an IP flooding bad keys is rejected before any DB lookup, and successful auth records nothing so a valid key is never limited.
    let client_ip = network::extract_client_ip(headers, Some(addr));
    if let Some(ip) = client_ip.as_deref() {
        if state.ingest_failure_limiter.is_over_budget(ip) {
            return Err(rate_limited_response_with_retry(60).into_response());
        }
    }

    let auth = match super::auth::authenticate(state, headers, uri, project_id).await {
        Ok(auth) => auth,
        Err(resp) => {
            // Only client-fault denials spend the budget; a 500 is our own DB outage.
            if is_client_fault(resp.status()) {
                if let Some(ip) = client_ip.as_deref() {
                    state.ingest_failure_limiter.record_failure(ip);
                }
            }
            return Err(resp);
        }
    };
    pre_filter(
        &state.filter_engine,
        headers,
        &auth.sentry_key,
        project_id,
        Some(addr),
    )?;
    Ok(auth)
}

/// Whether an auth rejection is the client's fault (bad/unknown key, archived, max projects) rather than an internal 500, which must not spend a budget.
fn is_client_fault(status: axum::http::StatusCode) -> bool {
    status != axum::http::StatusCode::INTERNAL_SERVER_ERROR
}

/// Run the event through the filter engine. Returns `true` when dropped, and
/// records the reason in discard_stats.
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

/// Pre-body checks (rate limits, user-agent blocks, IP blocks) against a
/// single filter snapshot. Rejects early if anything trips.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::failure_limiter::new_failure_limiter;
    use axum::http::StatusCode;

    #[test]
    fn only_internal_errors_are_exempt_from_the_budget() {
        assert!(!is_client_fault(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_client_fault(StatusCode::UNAUTHORIZED));
        assert!(is_client_fault(StatusCode::FORBIDDEN));
    }

    #[test]
    fn internal_errors_never_trip_the_limiter_but_denials_do() {
        let limiter = new_failure_limiter();
        for _ in 0..500 {
            if is_client_fault(StatusCode::INTERNAL_SERVER_ERROR) {
                limiter.record_failure("ip");
            }
        }
        assert!(!limiter.is_over_budget("ip"));
        for _ in 0..500 {
            if is_client_fault(StatusCode::UNAUTHORIZED) {
                limiter.record_failure("ip");
            }
        }
        assert!(limiter.is_over_budget("ip"));
    }
}
