use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;

use crate::endpoints::{
    authenticate_and_prefilter, check_event_filter, error_response, overloaded_response,
    sentry_response, sentry_response_with_discarded,
};
use crate::ingest::envelope;
use crate::ingest::models::{StorableAttachment, StorableEvent};
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

    // Large envelopes (up to max_body_size) parse JSON off the reactor, matching the writer's compression threshold.
    let parse_result = if body.len() > crate::util::INLINE_CPU_MAX_BYTES {
        crate::writer::block_in_place_if_multi_thread(|| envelope::parse(&body, project_id, &auth))
    } else {
        envelope::parse(&body, project_id, &auth)
    };
    let parsed = match parse_result {
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
    let mut filtered = 0usize;
    let mut pending_attachments = parsed.attachments;

    // Enrich, filter, and resolve attachments for every event first, so the whole envelope can queue all-or-nothing below; accepting a prefix and then 503-ing would make a retrying SDK re-send already-accepted events.
    let mut to_send: Vec<(StorableEvent, Vec<StorableAttachment>)> = Vec::new();
    for mut event in parsed.events {
        crate::ingest::enrich::enrich_event(&mut event);

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

        to_send.push((event, atts));
    }

    let accepted = to_send.len();
    if !state.writer.send_envelope(to_send) {
        return overloaded_response().into_response();
    }

    // Return 200 even when fully filtered: don't leak filter info to clients.
    if accepted == 0 && filtered > 0 {
        tracing::debug!("all {filtered} event(s) in envelope were filtered");
    }

    if filtered > 0 {
        return sentry_response_with_discarded(&event_id, filtered).into_response();
    }

    sentry_response(&event_id).into_response()
}
