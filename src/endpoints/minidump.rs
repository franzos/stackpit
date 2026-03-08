use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;

use crate::endpoints::{
    authenticate_and_prefilter, check_event_filter, error_response, overloaded_response,
    sentry_response,
};
use crate::models::StorableAttachment;
use crate::server::AppState;

pub async fn handle(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Path(project_id): Path<u64>,
    uri: Uri,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let auth = match authenticate_and_prefilter(&state, &headers, &uri, project_id, addr).await {
        Ok(a) => a,
        Err(resp) => return resp,
    };

    let event_id = uuid::Uuid::new_v4().to_string();
    let mut attachments = Vec::new();
    const MAX_MULTIPART_FIELDS: usize = 50;
    const MAX_FIELD_SIZE: usize = 20 * 1024 * 1024; // 20MB per field

    while let Ok(Some(field)) = multipart.next_field().await {
        if attachments.len() >= MAX_MULTIPART_FIELDS {
            tracing::warn!(
                "multipart field limit reached ({MAX_MULTIPART_FIELDS}), ignoring remaining fields"
            );
            break;
        }
        let name = field.name().unwrap_or("").to_string();
        let filename = field.file_name().unwrap_or("upload.dmp").to_string();
        let content_type = field.content_type().map(String::from);

        match field.bytes().await {
            Ok(data) if data.len() > MAX_FIELD_SIZE => {
                tracing::warn!("multipart field '{name}' exceeds {MAX_FIELD_SIZE} bytes, skipping");
                continue;
            }
            Ok(data) => {
                attachments.push(StorableAttachment {
                    event_id: event_id.clone(),
                    filename: if name == "upload_file_minidump" {
                        "minidump.dmp".to_string()
                    } else {
                        filename
                    },
                    content_type,
                    data: data.to_vec(),
                });
            }
            Err(e) => {
                tracing::warn!("multipart read error: {e}");
            }
        }
    }

    let mut event = match crate::envelope::parse_minidump(&event_id, project_id, &auth.sentry_key) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("failed to build minidump event: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "minidump event error")
                .into_response();
        }
    };
    crate::enrich::enrich_event(&mut event);

    if check_event_filter(&state, &event, project_id) {
        return sentry_response(&event_id).into_response();
    }

    if state
        .writer
        .send_event_with_attachments(event, attachments)
        .is_err()
    {
        return overloaded_response().into_response();
    }

    sentry_response(&event_id).into_response()
}
