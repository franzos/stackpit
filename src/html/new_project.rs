use askama::Template;
use axum::extract::{Form, State};
use serde::Deserialize;

use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::Chrome;
use crate::orgs::extractor::{require_org_owner, ActiveOrg};
use crate::queries;
use crate::server::AppState;

#[derive(Template)]
#[template(path = "new_project.html")]
struct NewProjectTemplate {
    message: Option<String>,
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
        let message = Some(chrome.t("flash-project-name-required"));
        let tmpl = NewProjectTemplate { message, chrome };
        return render_template(&tmpl);
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
        Ok((project_id, public_key)) => {
            let dsn = state.config.server.build_dsn(&public_key, project_id);
            let tmpl = ProjectCreatedTemplate {
                project_id,
                project_label: name.clone(),
                public_key,
                dsn,
                platform,
                chrome,
            };
            render_template(&tmpl)
        }
        Err(e) => {
            let tmpl = NewProjectTemplate {
                message: Some(chrome.err(e)),
                chrome,
            };
            render_template(&tmpl)
        }
    }
}
