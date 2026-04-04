use askama::Template;
use axum::extract::{Form, Path, State};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::forge;
use crate::html::render_template;
use crate::html::utils;
use crate::queries;
use crate::queries::types::{ProjectKey, ProjectRepo};
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

// askama needs these filters in scope for template derivation
#[allow(unused_imports)]
use crate::html::filters;

// ---------------------------------------------------------------------------
// General settings tab
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "project_settings.html")]
struct ProjectSettingsTemplate {
    project_id: u64,
    project_name: String,
    project_status: String,
    project_source: String,
    repos: Vec<ProjectRepo>,
    message: Option<String>,
    nav: ProjectNavCounts,
}

pub async fn handler(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    render_general(&state, project_id, None).await
}

#[derive(Deserialize)]
pub struct SetNameForm {
    pub name: String,
}

const MAX_FIELD_LENGTH: usize = 255;

pub async fn set_name(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Form(form): Form<SetNameForm>,
) -> axum::response::Response {
    let name = form.name.trim().to_string();
    if name.len() > MAX_FIELD_LENGTH {
        return render_general(
            &state,
            project_id,
            Some(format!(
                "Project name exceeds max length of {MAX_FIELD_LENGTH} characters"
            )),
        )
        .await;
    }

    let result = match utils::await_writer(state.writer.set_project_name(project_id, name)).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(()) => render_general(&state, project_id, Some("Project name updated".into())).await,
        Err(e) => render_general(&state, project_id, Some(format!("Error: {e}"))).await,
    }
}

#[derive(Deserialize)]
pub struct AddRepoForm {
    pub repo_url: String,
    pub url_template: Option<String>,
}

pub async fn add_repo(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Form(form): Form<AddRepoForm>,
) -> axum::response::Response {
    let repo_url = form.repo_url.trim().to_string();
    if repo_url.is_empty() {
        return render_general(
            &state,
            project_id,
            Some("Repository URL is required".into()),
        )
        .await;
    }
    if repo_url.len() > 2048 {
        return render_general(
            &state,
            project_id,
            Some("Repository URL exceeds max length of 2048 characters".into()),
        )
        .await;
    }

    let (forge_type, _) = forge::detect_forge(&repo_url);
    let url_template = form
        .url_template
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());

    let result = match utils::await_writer(state.writer.upsert_project_repo(
        project_id,
        repo_url,
        forge_type.as_str().to_string(),
        url_template,
    ))
    .await
    {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(()) => render_general(&state, project_id, Some("Repository added".into())).await,
        Err(e) => render_general(&state, project_id, Some(format!("Error: {e}"))).await,
    }
}

pub async fn delete_repo(
    State(state): State<AppState>,
    Path((project_id, repo_id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let result =
        match utils::await_writer(state.writer.delete_project_repo(project_id, repo_id)).await {
            Ok(r) => r,
            Err(resp) => return resp,
        };
    match result {
        Ok(()) => render_general(&state, project_id, Some("Repository removed".into())).await,
        Err(e) => render_general(&state, project_id, Some(format!("Error: {e}"))).await,
    }
}

pub async fn archive_project(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    let result = match utils::await_writer(state.writer.archive_project(project_id)).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(()) => {
            // Flush the auth cache for this project -- otherwise ingestion
            // would keep working until the cache entry expires.
            crate::auth_service::invalidate_project(&state.auth_cache, project_id);
            render_general(&state, project_id, Some("Project archived".into())).await
        }
        Err(e) => render_general(&state, project_id, Some(format!("Error: {e}"))).await,
    }
}

pub async fn unarchive_project(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    let result = match utils::await_writer(state.writer.unarchive_project(project_id)).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(()) => render_general(&state, project_id, Some("Project unarchived".into())).await,
        Err(e) => render_general(&state, project_id, Some(format!("Error: {e}"))).await,
    }
}

pub async fn delete_project(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    let result = match utils::await_writer(state.writer.delete_project(project_id)).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(()) => axum::response::Redirect::to("/web/projects/").into_response(),
        Err(e) => render_general(&state, project_id, Some(format!("Error: {e}"))).await,
    }
}

async fn render_general(
    state: &AppState,
    project_id: u64,
    message: Option<String>,
) -> axum::response::Response {
    let repos = queries::projects::get_project_repos(&state.pool, project_id)
        .await
        .unwrap_or_default();
    let info = queries::projects::get_project_info(&state.pool, project_id)
        .await
        .ok()
        .flatten();
    let project_name = info
        .as_ref()
        .and_then(|i| i.name.clone())
        .unwrap_or_default();
    let project_status = info
        .as_ref()
        .map(|i| i.status)
        .unwrap_or(crate::queries::types::ProjectStatus::Active)
        .to_string();
    let project_source = info
        .as_ref()
        .and_then(|i| i.source.clone())
        .unwrap_or_else(|| "auto".to_string());

    let nav = queries::projects::get_nav_counts(&state.pool, project_id).await;

    let tmpl = ProjectSettingsTemplate {
        project_id,
        project_name,
        project_status,
        project_source,
        repos,
        message,
        nav,
    };

    render_template(&tmpl)
}

// ---------------------------------------------------------------------------
// SDK Setup tab (keys)
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "project_settings_keys.html")]
struct ProjectKeysTemplate {
    project_id: u64,
    project_status: String,
    dsn: String,
    keys: Vec<ProjectKey>,
    message: Option<String>,
    nav: ProjectNavCounts,
}

pub async fn keys_handler(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    render_keys(&state, project_id, None).await
}

#[derive(Deserialize)]
pub struct CreateKeyForm {
    pub label: Option<String>,
}

pub async fn create_key(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Form(form): Form<CreateKeyForm>,
) -> axum::response::Response {
    let label = form
        .label
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());

    let result = match utils::await_writer(state.writer.create_project_key(project_id, label)).await
    {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(_key) => render_keys(&state, project_id, Some("Key created".into())).await,
        Err(e) => render_keys(&state, project_id, Some(format!("Error: {e}"))).await,
    }
}

pub async fn delete_key(
    State(state): State<AppState>,
    Path((project_id, public_key)): Path<(u64, String)>,
) -> axum::response::Response {
    let result =
        match utils::await_writer(state.writer.delete_project_key(public_key.clone())).await {
            Ok(r) => r,
            Err(resp) => return resp,
        };
    match result {
        Ok(()) => {
            crate::auth_service::invalidate_key(&state.auth_cache, &public_key);
            render_keys(&state, project_id, Some("Key deleted".into())).await
        }
        Err(e) => render_keys(&state, project_id, Some(format!("Error: {e}"))).await,
    }
}

async fn render_keys(
    state: &AppState,
    project_id: u64,
    message: Option<String>,
) -> axum::response::Response {
    let project_status = queries::projects::get_project_status(&state.pool, project_id)
        .await
        .unwrap_or(None)
        .unwrap_or(crate::queries::types::ProjectStatus::Active)
        .to_string();
    let keys = queries::projects::list_project_keys(&state.pool, project_id)
        .await
        .unwrap_or_default();

    let nav = queries::projects::get_nav_counts(&state.pool, project_id).await;

    let dsn = if let Some(first_key) = keys.first() {
        state
            .config
            .server
            .build_dsn(&first_key.public_key, project_id)
    } else {
        String::new()
    };

    let tmpl = ProjectKeysTemplate {
        project_id,
        project_status,
        dsn,
        keys,
        message,
        nav,
    };

    render_template(&tmpl)
}

// ---------------------------------------------------------------------------
// Source Maps tab
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "project_settings_sourcemaps.html")]
struct SourceMapsTemplate {
    project_id: u64,
    key_prefix: String,
    key_created_at: i64,
    new_key: String,
    message: Option<String>,
    sentry_url: String,
    nav: ProjectNavCounts,
}

pub async fn sourcemaps_handler(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    render_sourcemaps(&state, project_id, String::new(), None).await
}

pub async fn generate_sourcemap_key(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    let raw_key = {
        let mut buf = [0u8; 16];
        rand::fill(&mut buf);
        format!("spk_{}", hex::encode(buf))
    };

    let hash = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(raw_key.as_bytes()))
    };

    let prefix = &raw_key[..12];

    match queries::api_keys::create_api_key(&state.pool, project_id, "sourcemap", &hash, prefix)
        .await
    {
        Ok(()) => render_sourcemaps(&state, project_id, raw_key, None).await,
        Err(e) => {
            render_sourcemaps(
                &state,
                project_id,
                String::new(),
                Some(format!("Error: {e}")),
            )
            .await
        }
    }
}

async fn render_sourcemaps(
    state: &AppState,
    project_id: u64,
    new_key: String,
    message: Option<String>,
) -> axum::response::Response {
    let existing = queries::api_keys::get_api_key_for_project(&state.pool, project_id, "sourcemap")
        .await
        .unwrap_or(None);

    let nav = queries::projects::get_nav_counts(&state.pool, project_id).await;

    let sentry_url = format!("http://{}", state.config.server.bind);

    let tmpl = SourceMapsTemplate {
        project_id,
        key_prefix: existing
            .as_ref()
            .map(|k| k.key_prefix.clone())
            .unwrap_or_default(),
        key_created_at: existing.as_ref().map(|k| k.created_at).unwrap_or(0),
        new_key,
        message,
        sentry_url,
        nav,
    };

    render_template(&tmpl)
}
