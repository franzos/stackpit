use askama::Template;
use axum::extract::{Form, Path, State};
use serde::Deserialize;

use crate::html::render_template;
use crate::html::utils;
use crate::queries;
use crate::queries::alerts::{AlertRule, DigestSchedule};
use crate::server::AppState;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "alerts.html")]
struct AlertsTemplate {
    alert_rules: Vec<AlertRule>,
    digest_schedules: Vec<DigestSchedule>,
    message: Option<String>,
}

pub async fn handler(State(state): State<AppState>) -> axum::response::Response {
    render_page(&state, None).await
}

// -- Alert rules -------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAlertRuleForm {
    pub project_id: Option<String>,
    pub fingerprint: Option<String>,
    pub threshold_count: i64,
    pub window_secs: i64,
    pub cooldown_secs: Option<i64>,
}

pub async fn create_alert_rule(
    State(state): State<AppState>,
    Form(form): Form<CreateAlertRuleForm>,
) -> axum::response::Response {
    let project_id = form
        .project_id
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| s.trim().parse().ok());
    let fingerprint = form
        .fingerprint
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());

    let result = match utils::await_writer(state.writer.create_alert_rule(
        project_id,
        fingerprint,
        "threshold".to_string(),
        Some(form.threshold_count),
        Some(form.window_secs),
        form.cooldown_secs.unwrap_or(3600),
    ))
    .await
    {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(_id) => render_page(&state, Some("Alert rule created".into())).await,
        Err(e) => render_page(&state, Some(format!("Error: {e}"))).await,
    }
}

pub async fn delete_alert_rule(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> axum::response::Response {
    let result = match utils::await_writer(state.writer.delete_alert_rule(id)).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(()) => render_page(&state, Some("Alert rule deleted".into())).await,
        Err(e) => render_page(&state, Some(format!("Error: {e}"))).await,
    }
}

// -- Digest schedules --------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateDigestForm {
    pub project_id: Option<String>,
    pub interval_secs: i64,
}

pub async fn create_digest_schedule(
    State(state): State<AppState>,
    Form(form): Form<CreateDigestForm>,
) -> axum::response::Response {
    let project_id = form
        .project_id
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| s.trim().parse().ok());

    let result = match utils::await_writer(
        state
            .writer
            .create_digest_schedule(project_id, form.interval_secs),
    )
    .await
    {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(_id) => render_page(&state, Some("Digest schedule created".into())).await,
        Err(e) => render_page(&state, Some(format!("Error: {e}"))).await,
    }
}

pub async fn delete_digest_schedule(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> axum::response::Response {
    let result = match utils::await_writer(state.writer.delete_digest_schedule(id)).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(()) => render_page(&state, Some("Digest schedule deleted".into())).await,
        Err(e) => render_page(&state, Some(format!("Error: {e}"))).await,
    }
}

// -- Render ------------------------------------------------------------------

async fn render_page(state: &AppState, message: Option<String>) -> axum::response::Response {
    let alert_rules = queries::alerts::list_alert_rules(&state.pool, None)
        .await
        .unwrap_or_default();
    let digest_schedules = queries::alerts::list_digest_schedules(&state.pool)
        .await
        .unwrap_or_default();

    let tmpl = AlertsTemplate {
        alert_rules,
        digest_schedules,
        message,
    };

    render_template(&tmpl)
}
