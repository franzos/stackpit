use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::extractors::ApiReadPool;
use crate::queries;
use crate::server::AppState;

use super::{api_error, internal_error, json_or_500};

// -- Alert rules -------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAlertRuleBody {
    pub project_id: Option<u64>,
    pub fingerprint: Option<String>,
    pub trigger_kind: String,
    pub threshold_count: Option<i64>,
    pub window_secs: Option<i64>,
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: i64,
}

fn default_cooldown() -> i64 {
    3600
}

#[derive(Deserialize)]
pub struct UpdateAlertRuleBody {
    pub threshold_count: Option<i64>,
    pub window_secs: Option<i64>,
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// GET /api/v1/alerts/rules
pub async fn list_rules(ApiReadPool(pool): ApiReadPool) -> impl IntoResponse {
    json_or_500(
        queries::alerts::list_alert_rules(&pool, None)
            .await
            .map(|rules| {
                rules
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "id": r.id,
                            "project_id": r.project_id,
                            "fingerprint": r.fingerprint,
                            "trigger_kind": r.trigger_kind,
                            "threshold_count": r.threshold_count,
                            "window_secs": r.window_secs,
                            "cooldown_secs": r.cooldown_secs,
                            "enabled": r.enabled,
                            "created_at": r.created_at,
                        })
                    })
                    .collect::<Vec<_>>()
            }),
    )
}

/// POST /api/v1/alerts/rules
pub async fn create_rule(
    State(state): State<AppState>,
    Json(body): Json<CreateAlertRuleBody>,
) -> impl IntoResponse {
    let result = match crate::html::utils::await_writer(state.writer.create_alert_rule(
        body.project_id,
        body.fingerprint,
        body.trigger_kind,
        body.threshold_count,
        body.window_secs,
        body.cooldown_secs,
    ))
    .await
    {
        Ok(r) => r,
        Err(_) => return internal_error("writer unavailable").into_response(),
    };
    match result {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

/// PUT /api/v1/alerts/rules/{id}
pub async fn update_rule(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateAlertRuleBody>,
) -> impl IntoResponse {
    let result = match crate::html::utils::await_writer(state.writer.update_alert_rule(
        id,
        body.threshold_count,
        body.window_secs,
        body.cooldown_secs,
        body.enabled,
    ))
    .await
    {
        Ok(r) => r,
        Err(_) => return internal_error("writer unavailable").into_response(),
    };
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => api_error(StatusCode::NOT_FOUND, &e.to_string()).into_response(),
    }
}

/// DELETE /api/v1/alerts/rules/{id}
pub async fn delete_rule(State(state): State<AppState>, Path(id): Path<i64>) -> impl IntoResponse {
    let result = match crate::html::utils::await_writer(state.writer.delete_alert_rule(id)).await {
        Ok(r) => r,
        Err(_) => return internal_error("writer unavailable").into_response(),
    };
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => api_error(StatusCode::NOT_FOUND, &e.to_string()).into_response(),
    }
}

// -- Digest schedules --------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateDigestBody {
    pub project_id: Option<u64>,
    pub interval_secs: i64,
}

#[derive(Deserialize)]
pub struct UpdateDigestBody {
    pub interval_secs: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// GET /api/v1/digests
pub async fn list_digests(ApiReadPool(pool): ApiReadPool) -> impl IntoResponse {
    json_or_500(
        queries::alerts::list_digest_schedules(&pool)
            .await
            .map(|schedules| {
                schedules
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "id": s.id,
                            "project_id": s.project_id,
                            "interval_secs": s.interval_secs,
                            "last_sent": s.last_sent,
                            "enabled": s.enabled,
                            "created_at": s.created_at,
                        })
                    })
                    .collect::<Vec<_>>()
            }),
    )
}

/// POST /api/v1/digests
pub async fn create_digest(
    State(state): State<AppState>,
    Json(body): Json<CreateDigestBody>,
) -> impl IntoResponse {
    let result = match crate::html::utils::await_writer(
        state
            .writer
            .create_digest_schedule(body.project_id, body.interval_secs),
    )
    .await
    {
        Ok(r) => r,
        Err(_) => return internal_error("writer unavailable").into_response(),
    };
    match result {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

/// PUT /api/v1/digests/{id}
pub async fn update_digest(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateDigestBody>,
) -> impl IntoResponse {
    let result = match crate::html::utils::await_writer(state.writer.update_digest_schedule(
        id,
        body.interval_secs,
        body.enabled,
    ))
    .await
    {
        Ok(r) => r,
        Err(_) => return internal_error("writer unavailable").into_response(),
    };
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => api_error(StatusCode::NOT_FOUND, &e.to_string()).into_response(),
    }
}

/// DELETE /api/v1/digests/{id}
pub async fn delete_digest(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result =
        match crate::html::utils::await_writer(state.writer.delete_digest_schedule(id)).await {
            Ok(r) => r,
            Err(_) => return internal_error("writer unavailable").into_response(),
        };
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => api_error(StatusCode::NOT_FOUND, &e.to_string()).into_response(),
    }
}
