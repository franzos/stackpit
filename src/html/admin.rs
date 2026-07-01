use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use serde::Deserialize;

use crate::html::{html_error, render_template};
use crate::html::utils::Csrf;
use crate::orgs::extractor::{require_superuser, ActiveOrg};
use crate::queries::orgs::{list_non_system_orgs, OrgSummary};
use crate::queries::projects::{list_unassigned_projects, reassign_project, UnassignedProject};
use crate::server::AppState;

#[derive(Template)]
#[template(path = "admin_unassigned.html")]
struct UnassignedTemplate {
    projects: Vec<UnassignedProject>,
    orgs: Vec<OrgSummary>,
    csrf_token: String,
}

/// `GET /web/admin/unassigned`: superuser-only; lists projects still in the system org.
pub async fn unassigned_view(
    State(state): State<AppState>,
    active: ActiveOrg,
    Csrf(csrf): Csrf,
) -> axum::response::Response {
    if let Err(r) = require_superuser(&active) {
        return r;
    }
    let projects = match list_unassigned_projects(&state.pool).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("list_unassigned_projects failed: {e:#}");
            return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load projects.");
        }
    };
    let orgs = match list_non_system_orgs(&state.pool).await {
        Ok(o) => o,
        Err(e) => {
            tracing::error!("list_non_system_orgs failed: {e:#}");
            return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load orgs.");
        }
    };
    render_template(&UnassignedTemplate { projects, orgs, csrf_token: csrf })
}

#[derive(Deserialize)]
pub struct AssignForm {
    org_id: i64,
}

/// `POST /web/admin/projects/{id}/assign`: superuser-only; moves a project to the given org.
pub async fn assign_project(
    State(state): State<AppState>,
    active: ActiveOrg,
    Path(project_id): Path<i64>,
    Form(form): Form<AssignForm>,
) -> axum::response::Response {
    if let Err(r) = require_superuser(&active) {
        return r;
    }
    match reassign_project(&state.pool, project_id, form.org_id).await {
        Ok(_) => Redirect::to("/web/admin/unassigned").into_response(),
        Err(e) => {
            tracing::error!("reassign_project failed: {e:#}");
            html_error(StatusCode::INTERNAL_SERVER_ERROR, "Reassignment failed.")
        }
    }
}
