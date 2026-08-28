use askama::Template;
use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::extractors::ProjectPath;
use crate::html::chrome::PageChrome;
use crate::html::flash::{self, Flash};
use crate::html::utils::Chrome;
use crate::html::{render_template, HtmlError};
use crate::orgs::extractor::{require_org_owner, require_project_scope, ActiveOrg};
use crate::queries;
use crate::server::AppState;

#[derive(Template)]
#[template(path = "new_project.html")]
struct NewProjectTemplate {
    message: Option<Flash>,
    chrome: PageChrome,
}

#[derive(Template)]
#[template(path = "project_created.html")]
struct ProjectCreatedTemplate {
    project_id: u64,
    project_label: String,
    public_key: String,
    dsn: String,
    platform: String,
    chrome: PageChrome,
}

pub async fn form(Chrome(chrome): Chrome) -> axum::response::Response {
    let tmpl = NewProjectTemplate {
        message: None,
        chrome,
    };
    render_template(&tmpl)
}

#[derive(Deserialize)]
pub struct CreateProjectForm {
    pub name: String,
    pub platform: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active_org: ActiveOrg,
    Form(form): Form<CreateProjectForm>,
) -> axum::response::Response {
    if let Err(resp) = require_org_owner(&active_org) {
        return resp;
    }

    let name = form.name.trim().to_string();
    if name.is_empty() {
        return flash::redirect("/web/projects/new", flash::PROJECT_NAME_REQUIRED);
    }

    let platform = form
        .platform
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let result = queries::projects::create_project(
        &state.writer_pool,
        active_org.session_org_id,
        &name,
        if platform.is_empty() {
            None
        } else {
            Some(platform.as_str())
        },
    )
    .await;
    match result {
        // The DSN page is not the submitted form, so there is no naive PRG
        // target, and the public key must never travel in a query string.
        // Redirect to a route that rebuilds the page from the database instead:
        // refresh is idempotent and the operator can come back to the URL.
        Ok((project_id, _public_key)) => {
            axum::response::Redirect::to(&format!("/web/projects/{project_id}/created"))
                .into_response()
        }
        // A create failure has no enumerable key — the database said something
        // specific — so this one renders in place rather than redirecting.
        Err(e) => {
            let tmpl = NewProjectTemplate {
                message: Some(Flash::err(chrome.err(e))),
                chrome,
            };
            render_template(&tmpl)
        }
    }
}

/// The DSN page for a project that already exists. Behind the project-scope
/// gate, so it discloses nothing the settings pages don't.
pub async fn created(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
) -> Result<axum::response::Response, HtmlError> {
    if require_project_scope(&active, &state.pool, project_id as i64)
        .await
        .is_err()
    {
        return Err(HtmlError(StatusCode::NOT_FOUND, "Not found".into()));
    }

    let info = queries::projects::get_project_info(&state.pool, project_id)
        .await
        .ok()
        .flatten()
        .ok_or_else(|| HtmlError(StatusCode::NOT_FOUND, "Project not found".into()))?;

    let keys = queries::projects::list_project_keys(&state.pool, project_id)
        .await
        .unwrap_or_default();
    // `create_project` stores the chosen platform as the first key's label.
    let key = keys
        .first()
        .ok_or_else(|| HtmlError(StatusCode::NOT_FOUND, "Project has no key".into()))?;

    let tmpl = ProjectCreatedTemplate {
        project_id,
        project_label: info.name.clone().unwrap_or_else(|| project_id.to_string()),
        dsn: state.config.server.build_dsn(&key.public_key, project_id),
        public_key: key.public_key.clone(),
        platform: key.label.clone().unwrap_or_default(),
        chrome,
    };
    Ok(render_template(&tmpl))
}
