use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;

use crate::auth;
use crate::auth::SentryAuth;
use crate::auth_service::{self, AuthError};
use crate::server::AppState;

use super::error_response;

/// Pulls auth credentials from headers or query params, then validates against
/// the DB. I'm using a DashMap cache here so we don't hammer the DB on every request.
#[allow(clippy::result_large_err)]
pub async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
    project_id: u64,
) -> Result<SentryAuth, axum::response::Response> {
    let auth = match auth::extract_from_header(headers)
        .or_else(|| auth::extract_from_query(uri.query()))
    {
        Some(a) => a,
        None => {
            return Err(
                error_response(StatusCode::UNAUTHORIZED, "missing sentry key").into_response(),
            )
        }
    };

    match auth_service::validate_project_key(state, &auth.sentry_key, project_id).await {
        Ok(()) => Ok(auth),
        Err(AuthError::Archived) => {
            Err(error_response(StatusCode::FORBIDDEN, "project is archived").into_response())
        }
        Err(AuthError::Denied(msg)) => {
            Err(error_response(StatusCode::FORBIDDEN, msg).into_response())
        }
        Err(AuthError::MaxProjects) => {
            Err(error_response(StatusCode::FORBIDDEN, "max projects reached").into_response())
        }
        Err(AuthError::InternalError) => {
            Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response())
        }
    }
}
