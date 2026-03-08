use askama::Template;
use axum::extract::{Form, Path, State};
use serde::Deserialize;

use crate::html::render_template;
use crate::html::utils;
use crate::queries;
use crate::queries::types::{Integration, ProjectIntegration};
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "project_integrations.html")]
struct ProjectIntegrationsTemplate {
    project_id: u64,
    project_status: String,
    active: Vec<ProjectIntegration>,
    available: Vec<Integration>,
    message: Option<String>,
    nav: ProjectNavCounts,
}

pub async fn handler(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    render_page(&state, project_id, None).await
}

#[derive(Deserialize)]
pub struct ActivateForm {
    pub integration_id: i64,
    pub notify_new_issues: Option<String>,
    pub notify_regressions: Option<String>,
    pub min_level: Option<String>,
    pub environment_filter: Option<String>,
    pub to_address: Option<String>,
    pub notify_threshold: Option<String>,
    pub notify_digests: Option<String>,
}

pub async fn activate(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Form(form): Form<ActivateForm>,
) -> axum::response::Response {
    let config = form
        .to_address
        .filter(|s| !s.trim().is_empty())
        .map(|s| serde_json::json!({ "to": s.trim() }).to_string());

    let result = match utils::await_writer(state.writer.activate_project_integration(
        project_id,
        form.integration_id,
        form.notify_new_issues.is_some(),
        form.notify_regressions.is_some(),
        form.min_level.filter(|s| !s.is_empty()),
        form.environment_filter.filter(|s| !s.trim().is_empty()),
        config,
        form.notify_threshold.is_some(),
        form.notify_digests.is_some(),
    ))
    .await
    {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(()) => render_page(&state, project_id, Some("Integration activated".into())).await,
        Err(e) => render_page(&state, project_id, Some(format!("Error: {e}"))).await,
    }
}

#[derive(Deserialize)]
pub struct UpdateForm {
    pub notify_new_issues: Option<String>,
    pub notify_regressions: Option<String>,
    pub min_level: Option<String>,
    pub environment_filter: Option<String>,
    pub to_address: Option<String>,
    pub notify_threshold: Option<String>,
    pub notify_digests: Option<String>,
}

pub async fn update(
    State(state): State<AppState>,
    Path((project_id, id)): Path<(u64, i64)>,
    Form(form): Form<UpdateForm>,
) -> axum::response::Response {
    let config = form
        .to_address
        .filter(|s| !s.trim().is_empty())
        .map(|s| serde_json::json!({ "to": s.trim() }).to_string());

    let result = match utils::await_writer(state.writer.update_project_integration(
        id,
        form.notify_new_issues.is_some(),
        form.notify_regressions.is_some(),
        form.min_level.filter(|s| !s.is_empty()),
        form.environment_filter.filter(|s| !s.trim().is_empty()),
        config,
        form.notify_threshold.is_some(),
        form.notify_digests.is_some(),
    ))
    .await
    {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(()) => render_page(&state, project_id, Some("Integration updated".into())).await,
        Err(e) => render_page(&state, project_id, Some(format!("Error: {e}"))).await,
    }
}

pub async fn deactivate(
    State(state): State<AppState>,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let result = match utils::await_writer(state.writer.deactivate_project_integration(id)).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(()) => render_page(&state, project_id, Some("Integration deactivated".into())).await,
        Err(e) => render_page(&state, project_id, Some(format!("Error: {e}"))).await,
    }
}

async fn render_page(
    state: &AppState,
    project_id: u64,
    message: Option<String>,
) -> axum::response::Response {
    let project_status = queries::projects::get_project_status(&state.pool, project_id)
        .await
        .unwrap_or(None)
        .unwrap_or(crate::queries::types::ProjectStatus::Active)
        .to_string();
    let active = queries::integrations::list_project_integrations(&state.pool, project_id)
        .await
        .unwrap_or_default();
    let available = queries::integrations::list_available_for_project(&state.pool, project_id)
        .await
        .unwrap_or_default();

    let nav = queries::projects::get_nav_counts(&state.pool, project_id).await;

    let tmpl = ProjectIntegrationsTemplate {
        project_id,
        project_status,
        active,
        available,
        message,
        nav,
    };

    render_template(&tmpl)
}
