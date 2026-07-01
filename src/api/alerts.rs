use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::extractors::ReadPool;
use crate::orgs::extractor::{require_owner, ActiveOrg};
use crate::queries;
use crate::server::AppState;

use super::ApiError;

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
pub async fn list_rules(
    active: ActiveOrg,
    ReadPool(pool): ReadPool,
) -> Result<impl IntoResponse, ApiError> {
    let org_scope = if active.role.is_none() { None } else { Some(active.org_id) };
    let rules = queries::alerts::list_alert_rules(&pool, None, org_scope)
        .await
        .map_err(ApiError::internal)?;
    let out = rules
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
        .collect::<Vec<_>>();
    Ok(Json(out))
}

/// POST /api/v1/alerts/rules
pub async fn create_rule(
    active: ActiveOrg,
    State(state): State<AppState>,
    Json(body): Json<CreateAlertRuleBody>,
) -> Result<impl IntoResponse, ApiError> {
    require_owner(&active).map_err(|_| ApiError::new(StatusCode::FORBIDDEN, "forbidden"))?;
    if let Some(pid) = body.project_id {
        if active.role.is_some() {
            crate::queries::orgs::assert_project_in_org(
                &state.pool,
                pid as i64,
                active.org_id,
            )
            .await
            .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "project not in org"))?;
        }
    }
    let id = queries::alerts::create_alert_rule(
        &state.writer_pool,
        active.org_id,
        body.project_id,
        body.fingerprint.as_deref(),
        &body.trigger_kind,
        body.threshold_count,
        body.window_secs,
        body.cooldown_secs,
    )
    .await
    .map_err(ApiError::internal)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

/// PUT /api/v1/alerts/rules/{id}
pub async fn update_rule(
    active: ActiveOrg,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateAlertRuleBody>,
) -> Result<impl IntoResponse, ApiError> {
    require_owner(&active).map_err(|_| ApiError::new(StatusCode::FORBIDDEN, "forbidden"))?;
    match queries::alerts::update_alert_rule(
        &state.writer_pool,
        id,
        active.org_id,
        body.threshold_count,
        body.window_secs,
        body.cooldown_secs,
        body.enabled,
    )
    .await
    {
        Ok(0) => Err(ApiError::not_found(format!("not found: alert rule: {id}"))),
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err(ApiError::not_found(e.to_string())),
    }
}

/// DELETE /api/v1/alerts/rules/{id}
pub async fn delete_rule(
    active: ActiveOrg,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    require_owner(&active).map_err(|_| ApiError::new(StatusCode::FORBIDDEN, "forbidden"))?;
    match queries::alerts::delete_alert_rule(&state.writer_pool, id, active.org_id).await {
        Ok(0) => Err(ApiError::not_found(format!("not found: alert rule: {id}"))),
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err(ApiError::not_found(e.to_string())),
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
pub async fn list_digests(
    active: ActiveOrg,
    ReadPool(pool): ReadPool,
) -> Result<impl IntoResponse, ApiError> {
    let org_scope = if active.role.is_none() { None } else { Some(active.org_id) };
    let schedules = queries::alerts::list_digest_schedules(&pool, org_scope)
        .await
        .map_err(ApiError::internal)?;
    let out = schedules
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
        .collect::<Vec<_>>();
    Ok(Json(out))
}

/// POST /api/v1/digests
pub async fn create_digest(
    active: ActiveOrg,
    State(state): State<AppState>,
    Json(body): Json<CreateDigestBody>,
) -> Result<impl IntoResponse, ApiError> {
    require_owner(&active).map_err(|_| ApiError::new(StatusCode::FORBIDDEN, "forbidden"))?;
    if let Some(pid) = body.project_id {
        if active.role.is_some() {
            crate::queries::orgs::assert_project_in_org(
                &state.pool,
                pid as i64,
                active.org_id,
            )
            .await
            .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "project not in org"))?;
        }
    }
    let id = queries::alerts::create_digest_schedule(
        &state.writer_pool,
        active.org_id,
        body.project_id,
        body.interval_secs,
    )
    .await
    .map_err(ApiError::internal)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

/// PUT /api/v1/digests/{id}
pub async fn update_digest(
    active: ActiveOrg,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateDigestBody>,
) -> Result<impl IntoResponse, ApiError> {
    require_owner(&active).map_err(|_| ApiError::new(StatusCode::FORBIDDEN, "forbidden"))?;
    match queries::alerts::update_digest_schedule(
        &state.writer_pool,
        id,
        active.org_id,
        body.interval_secs,
        body.enabled,
    )
    .await
    {
        Ok(0) => Err(ApiError::not_found(format!(
            "not found: digest schedule: {id}"
        ))),
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err(ApiError::not_found(e.to_string())),
    }
}

/// DELETE /api/v1/digests/{id}
pub async fn delete_digest(
    active: ActiveOrg,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    require_owner(&active).map_err(|_| ApiError::new(StatusCode::FORBIDDEN, "forbidden"))?;
    match queries::alerts::delete_digest_schedule(&state.writer_pool, id, active.org_id).await {
        Ok(0) => Err(ApiError::not_found(format!(
            "not found: digest schedule: {id}"
        ))),
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err(ApiError::not_found(e.to_string())),
    }
}
