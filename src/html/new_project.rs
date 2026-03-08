use askama::Template;
use axum::extract::{Form, State};
use serde::Deserialize;

use crate::html::render_template;
use crate::html::utils;
use crate::server::AppState;

#[derive(Template)]
#[template(path = "new_project.html")]
struct NewProjectTemplate {
    message: Option<String>,
}

#[derive(Template)]
#[template(path = "project_created.html")]
struct ProjectCreatedTemplate {
    project_id: u64,
    public_key: String,
    dsn: String,
    platform: String,
}

pub async fn form() -> axum::response::Response {
    let tmpl = NewProjectTemplate { message: None };
    render_template(&tmpl)
}

#[derive(Deserialize)]
pub struct CreateProjectForm {
    pub name: String,
    pub platform: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    Form(form): Form<CreateProjectForm>,
) -> axum::response::Response {
    let name = form.name.trim().to_string();
    if name.is_empty() {
        let tmpl = NewProjectTemplate {
            message: Some("Project name is required".into()),
        };
        return render_template(&tmpl);
    }

    let platform = form
        .platform
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let result = match utils::await_writer(state.writer.create_project(
        name.clone(),
        if platform.is_empty() {
            None
        } else {
            Some(platform.clone())
        },
    ))
    .await
    {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok((project_id, public_key)) => {
            let dsn = state.config.server.build_dsn(&public_key, project_id);
            let tmpl = ProjectCreatedTemplate {
                project_id,
                public_key,
                dsn,
                platform,
            };
            render_template(&tmpl)
        }
        Err(e) => {
            let tmpl = NewProjectTemplate {
                message: Some(format!("Error: {e}")),
            };
            render_template(&tmpl)
        }
    }
}
