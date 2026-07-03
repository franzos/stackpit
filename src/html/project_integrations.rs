use askama::Template;
use axum::extract::{Form, Path, State};
use serde::Deserialize;

use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::{self, Chrome};
use crate::orgs::extractor::{require_owner, require_project_scope, ActiveOrg};
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
    active: Vec<ProjectIntegration>,
    available: Vec<Integration>,
    message: Option<String>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn handler(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    render_page(&state, project_id, None, &chrome, active.org_id).await
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
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    Path(project_id): Path<u64>,
    Form(form): Form<ActivateForm>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }
    // Reject cross-org links: the integration must belong to the active org.
    match queries::integrations::get_integration(
        &state.pool,
        form.integration_id,
        Some(active.org_id),
    )
    .await
    {
        Ok(Some(_)) => {}
        Ok(None) => {
            return render_page(
                &state,
                project_id,
                Some(chrome.t("flash-integration-not-found")),
                &chrome,
                active.org_id,
            )
            .await;
        }
        Err(e) => {
            return render_page(
                &state,
                project_id,
                Some(chrome.err(e)),
                &chrome,
                active.org_id,
            )
            .await;
        }
    }
    let config = form
        .to_address
        .filter(|s| !s.trim().is_empty())
        .map(|s| serde_json::json!({ "to": s.trim() }).to_string());

    let s = state.clone();
    let org_id = active.org_id;
    let success = chrome.t("flash-integration-activated");
    let render_chrome = chrome.clone();
    utils::query_then_render(
        queries::integrations::activate_project_integration(
            &state.writer_pool,
            project_id,
            form.integration_id,
            form.notify_new_issues.is_some(),
            form.notify_regressions.is_some(),
            form.min_level.filter(|s| !s.is_empty()).as_deref(),
            form.environment_filter
                .filter(|s| !s.trim().is_empty())
                .as_deref(),
            config.as_deref(),
            form.notify_threshold.is_some(),
            form.notify_digests.is_some(),
        )
        .await,
        &chrome,
        &success,
        move |msg| async move { render_page(&s, project_id, msg, &render_chrome, org_id).await },
    )
    .await
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
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    Path((project_id, id)): Path<(u64, i64)>,
    Form(form): Form<UpdateForm>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }
    let config = form
        .to_address
        .filter(|s| !s.trim().is_empty())
        .map(|s| serde_json::json!({ "to": s.trim() }).to_string());

    let msg = match queries::integrations::update_project_integration(
        &state.writer_pool,
        project_id as i64,
        id,
        form.notify_new_issues.is_some(),
        form.notify_regressions.is_some(),
        form.min_level.filter(|s| !s.is_empty()).as_deref(),
        form.environment_filter
            .filter(|s| !s.trim().is_empty())
            .as_deref(),
        config.as_deref(),
        form.notify_threshold.is_some(),
        form.notify_digests.is_some(),
    )
    .await
    {
        Ok(0) => format!(
            "{} {}",
            chrome.t("common-error-prefix"),
            chrome.tv1("flash-not-found-project-integration", "id", &id.to_string())
        ),
        Ok(_) => chrome.t("flash-integration-updated"),
        Err(e) => chrome.err(e),
    };
    render_page(&state, project_id, Some(msg), &chrome, active.org_id).await
}

pub async fn deactivate(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }
    let msg = match queries::integrations::deactivate_project_integration(
        &state.writer_pool,
        project_id as i64,
        id,
    )
    .await
    {
        Ok(0) => format!(
            "{} {}",
            chrome.t("common-error-prefix"),
            chrome.tv1("flash-not-found-project-integration", "id", &id.to_string())
        ),
        Ok(_) => chrome.t("flash-integration-deactivated"),
        Err(e) => chrome.err(e),
    };
    render_page(&state, project_id, Some(msg), &chrome, active.org_id).await
}

async fn render_page(
    state: &AppState,
    project_id: u64,
    message: Option<String>,
    chrome: &PageChrome,
    org_id: i64,
) -> axum::response::Response {
    let active = queries::integrations::list_project_integrations(&state.pool, project_id)
        .await
        .unwrap_or_default();
    let available =
        queries::integrations::list_available_for_project(&state.pool, project_id, org_id)
            .await
            .unwrap_or_default();

    let nav = state.nav_counts(project_id).await;

    let tmpl = ProjectIntegrationsTemplate {
        project_id,
        active,
        available,
        message,
        nav,
        chrome: chrome.clone(),
    };

    render_template(&tmpl)
}
