use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;

use crate::endpoints::pipeline::{authenticate_and_prefilter, check_event_filter};
use crate::endpoints::responses::{error_response, overloaded_response, sentry_response};
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

    let mut event = match envelope::parse_store_body(&body, project_id, &auth) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("invalid event payload: {e}");
            return error_response(StatusCode::BAD_REQUEST, "invalid event payload")
                .into_response();
        }
    };
    crate::enrich::enrich_event(&mut event);

    if check_event_filter(&state, &event, project_id) {
        return sentry_response(&event.event_id).into_response();
    }

    let event_id = event.event_id.clone();
    if state.writer.send_event(event).is_err() {
        return overloaded_response().into_response();
    }

    sentry_response(&event_id).into_response()
}
