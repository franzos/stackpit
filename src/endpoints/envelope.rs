use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;

use crate::endpoints::{
    authenticate_and_prefilter, check_event_filter, error_response, overloaded_response,
    sentry_response, sentry_response_with_discarded,
};
use crate::envelope;
use crate::server::AppState;

pub async fn handle(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Path(project_id): Path<u64>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let auth = match authenticate_and_prefilter(&state, &headers, &uri, project_id, addr).await {
        Ok(a) => a,
        Err(resp) => return resp,
    };

    let parsed = match envelope::parse(&body, project_id, &auth) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("envelope parse error: {e}");
            return error_response(StatusCode::BAD_REQUEST, "invalid envelope").into_response();
        }
    };

    let event_id = parsed
        .events
        .first()
        .map(|e| e.event_id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let event_count = parsed.events.len();
    let mut accepted = 0usize;
    let mut filtered = 0usize;
    let mut pending_attachments = parsed.attachments;

    for mut event in parsed.events {
        crate::enrich::enrich_event(&mut event);

        if check_event_filter(&state, &event, project_id) {
            filtered += 1;
            continue;
        }

        // Sentry's spec ties attachments to the envelope-level event_id.
        // If there's an explicit ID, only the matching event gets them.
        // No ID + single event? That event gets everything. Multi-event
        // envelopes without an ID? Nobody gets attachments.
        let is_target = match &parsed.envelope_event_id {
            Some(eid) => *eid == event.event_id,
            None => event_count == 1,
        };

        let atts: Vec<_> = if is_target {
            pending_attachments
                .drain(..)
                .map(|mut a| {
                    a.event_id = event.event_id.clone();
                    a
                })
                .collect()
        } else {
            Vec::new()
        };

        if state
            .writer
            .send_event_with_attachments(event, atts)
            .is_err()
        {
            return overloaded_response().into_response();
        }
        accepted += 1;
    }

    // Everything in this envelope got filtered out. We still return 200 --
    // leaking filter info to the client would be a bad idea. The discards
    // are tracked in discard_stats so we can surface them in the UI.
    if accepted == 0 && filtered > 0 {
        tracing::debug!("all {filtered} event(s) in envelope were filtered");
    }

    if filtered > 0 {
        return sentry_response_with_discarded(&event_id, filtered).into_response();
    }

    sentry_response(&event_id).into_response()
}
