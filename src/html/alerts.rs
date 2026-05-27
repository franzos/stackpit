use askama::Template;
use axum::extract::{Form, Path, State};
use serde::Deserialize;

use crate::html::render_template;
use crate::html::utils::{self, Csrf};
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
    csrf_token: String,
}

pub async fn handler(State(state): State<AppState>, Csrf(csrf): Csrf) -> axum::response::Response {
    render_page(&state, None, &csrf).await
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
    Csrf(csrf): Csrf,
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

    let s = state.clone();
    utils::query_then_render(
        queries::alerts::create_alert_rule(
            &state.writer_pool,
            project_id,
            fingerprint.as_deref(),
            "threshold",
            Some(form.threshold_count),
            Some(form.window_secs),
            form.cooldown_secs.unwrap_or(3600),
        )
        .await,
        "Alert rule created",
        move |msg| async move { render_page(&s, msg, &csrf).await },
    )
    .await
}

pub async fn delete_alert_rule(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path(id): Path<i64>,
) -> axum::response::Response {
    let msg = match queries::alerts::delete_alert_rule(&state.writer_pool, id).await {
        Ok(0) => format!("Error: not found: alert rule: {id}"),
        Ok(_) => "Alert rule deleted".to_string(),
        Err(e) => format!("Error: {e}"),
    };
    render_page(&state, Some(msg), &csrf).await
}

// -- Digest schedules --------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateDigestForm {
    pub project_id: Option<String>,
    pub interval_secs: i64,
}

pub async fn create_digest_schedule(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Form(form): Form<CreateDigestForm>,
) -> axum::response::Response {
    let project_id = form
        .project_id
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| s.trim().parse().ok());

    let s = state.clone();
    utils::query_then_render(
        queries::alerts::create_digest_schedule(&state.writer_pool, project_id, form.interval_secs)
            .await,
        "Digest schedule created",
        move |msg| async move { render_page(&s, msg, &csrf).await },
    )
    .await
}

pub async fn delete_digest_schedule(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path(id): Path<i64>,
) -> axum::response::Response {
    let msg = match queries::alerts::delete_digest_schedule(&state.writer_pool, id).await {
        Ok(0) => format!("Error: not found: digest schedule: {id}"),
        Ok(_) => "Digest schedule deleted".to_string(),
        Err(e) => format!("Error: {e}"),
    };
    render_page(&state, Some(msg), &csrf).await
}

// -- Render ------------------------------------------------------------------

async fn render_page(
    state: &AppState,
    message: Option<String>,
    csrf: &str,
) -> axum::response::Response {
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
        csrf_token: csrf.to_string(),
    };

    render_template(&tmpl)
}
