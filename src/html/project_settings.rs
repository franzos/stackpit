use askama::Template;
use axum::extract::{Form, Path, State};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::forge;
use crate::html::render_template;
use crate::html::utils::{self, Csrf};
use crate::orgs::extractor::{require_owner, require_project_scope, ActiveOrg};
use crate::queries;
use crate::queries::types::{ProjectKey, ProjectRepo};
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

// askama needs these filters in scope for template derivation
#[allow(unused_imports)]
use crate::html::filters;

// General settings tab

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
    csrf_token: String,
}

pub async fn handler(
    State(state): State<AppState>,
    active: ActiveOrg,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    render_general(&state, project_id, None, &csrf).await
}

#[derive(Deserialize)]
pub struct SetNameForm {
    pub name: String,
}

const MAX_FIELD_LENGTH: usize = 255;

pub async fn set_name(
    State(state): State<AppState>,
    active: ActiveOrg,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Form(form): Form<SetNameForm>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }

    let name = form.name.trim().to_string();
    if name.len() > MAX_FIELD_LENGTH {
        return render_general(
            &state,
            project_id,
            Some(format!(
                "Project name exceeds max length of {MAX_FIELD_LENGTH} characters"
            )),
            &csrf,
        )
        .await;
    }

    let s = state.clone();
    utils::query_then_render(
        queries::projects::set_project_name(&state.writer_pool, project_id, &name).await,
        "Project name updated",
        move |msg| async move { render_general(&s, project_id, msg, &csrf).await },
    )
    .await
}

#[derive(Deserialize)]
pub struct AddRepoForm {
    pub repo_url: String,
    pub url_template: Option<String>,
}

pub async fn add_repo(
    State(state): State<AppState>,
    active: ActiveOrg,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Form(form): Form<AddRepoForm>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }

    let repo_url = form.repo_url.trim().to_string();
    if repo_url.is_empty() {
        return render_general(
            &state,
            project_id,
            Some("Repository URL is required".into()),
            &csrf,
        )
        .await;
    }
    if repo_url.len() > 2048 {
        return render_general(
            &state,
            project_id,
            Some("Repository URL exceeds max length of 2048 characters".into()),
            &csrf,
        )
        .await;
    }

    let (forge_type, _) = forge::detect_forge(&repo_url);
    let url_template = form
        .url_template
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());

    let s = state.clone();
    utils::query_then_render(
        queries::projects::upsert_project_repo(
            &state.writer_pool,
            project_id,
            &repo_url,
            forge_type.as_str(),
            url_template.as_deref(),
        )
        .await,
        "Repository added",
        move |msg| async move { render_general(&s, project_id, msg, &csrf).await },
    )
    .await
}

pub async fn delete_repo(
    State(state): State<AppState>,
    active: ActiveOrg,
    Csrf(csrf): Csrf,
    Path((project_id, repo_id)): Path<(u64, i64)>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }

    let msg = match queries::projects::delete_project_repo(&state.writer_pool, project_id, repo_id)
        .await
    {
        Ok(0) => format!("Error: not found: repo: {repo_id}"),
        Ok(_) => "Repository removed".to_string(),
        Err(e) => format!("Error: {e}"),
    };
    render_general(&state, project_id, Some(msg), &csrf).await
}

pub async fn archive_project(
    State(state): State<AppState>,
    active: ActiveOrg,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }

    match queries::projects::archive_project(&state.writer_pool, project_id).await {
        Ok(0) => {
            render_general(
                &state,
                project_id,
                Some(format!("Error: not found: project: {project_id}")),
                &csrf,
            )
            .await
        }
        Ok(_) => {
            // Flush the auth cache or ingestion keeps working until the entry expires.
            crate::ingest::auth::invalidate_project(&state.auth_cache, project_id);
            render_general(&state, project_id, Some("Project archived".into()), &csrf).await
        }
        Err(e) => render_general(&state, project_id, Some(format!("Error: {e}")), &csrf).await,
    }
}

pub async fn unarchive_project(
    State(state): State<AppState>,
    active: ActiveOrg,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }

    let msg = match queries::projects::unarchive_project(&state.writer_pool, project_id).await {
        Ok(0) => format!("Error: not found: project: {project_id}"),
        Ok(_) => "Project unarchived".to_string(),
        Err(e) => format!("Error: {e}"),
    };
    render_general(&state, project_id, Some(msg), &csrf).await
}

pub async fn delete_project(
    State(state): State<AppState>,
    active: ActiveOrg,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }

    match queries::projects::delete_project(&state.writer_pool, project_id).await {
        Ok(()) => axum::response::Redirect::to("/web/projects/").into_response(),
        Err(e) => render_general(&state, project_id, Some(format!("Error: {e}")), &csrf).await,
    }
}

async fn render_general(
    state: &AppState,
    project_id: u64,
    message: Option<String>,
    csrf: &str,
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
        .unwrap_or(crate::domain::ProjectStatus::Active)
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
        csrf_token: csrf.to_string(),
    };

    render_template(&tmpl)
}

// SDK Setup tab (keys)

#[derive(Template)]
#[template(path = "project_settings_keys.html")]
struct ProjectKeysTemplate {
    project_id: u64,
    dsn: String,
    keys: Vec<ProjectKey>,
    message: Option<String>,
    nav: ProjectNavCounts,
    csrf_token: String,
}

pub async fn keys_handler(
    State(state): State<AppState>,
    active: ActiveOrg,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    render_keys(&state, project_id, None, &csrf).await
}

#[derive(Deserialize)]
pub struct CreateKeyForm {
    pub label: Option<String>,
}

pub async fn create_key(
    State(state): State<AppState>,
    active: ActiveOrg,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Form(form): Form<CreateKeyForm>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }

    let label = form
        .label
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());

    let s = state.clone();
    utils::query_then_render(
        queries::projects::create_project_key(&state.writer_pool, project_id, label.as_deref())
            .await,
        "Key created",
        move |msg| async move { render_keys(&s, project_id, msg, &csrf).await },
    )
    .await
}

pub async fn delete_key(
    State(state): State<AppState>,
    active: ActiveOrg,
    Csrf(csrf): Csrf,
    Path((project_id, public_key)): Path<(u64, String)>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }

    match queries::projects::delete_project_key(&state.writer_pool, project_id, &public_key).await {
        Ok(0) => {
            render_keys(
                &state,
                project_id,
                Some(format!("Error: not found: key: {public_key}")),
                &csrf,
            )
            .await
        }
        Ok(_) => {
            crate::ingest::auth::invalidate_key(&state.auth_cache, &public_key);
            render_keys(&state, project_id, Some("Key deleted".into()), &csrf).await
        }
        Err(e) => render_keys(&state, project_id, Some(format!("Error: {e}")), &csrf).await,
    }
}

async fn render_keys(
    state: &AppState,
    project_id: u64,
    message: Option<String>,
    csrf: &str,
) -> axum::response::Response {
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
        dsn,
        keys,
        message,
        nav,
        csrf_token: csrf.to_string(),
    };

    render_template(&tmpl)
}

// Source Maps tab

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
    csrf_token: String,
}

pub async fn sourcemaps_handler(
    State(state): State<AppState>,
    active: ActiveOrg,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    render_sourcemaps(&state, project_id, String::new(), None, &csrf).await
}

pub async fn generate_sourcemap_key(
    State(state): State<AppState>,
    active: ActiveOrg,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }

    let raw_key = format!("spk_{}", crate::util::crypto::random_hex::<16>());

    let hash = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(raw_key.as_bytes()))
    };

    let prefix = &raw_key[..12];

    match queries::api_keys::create_api_key(&state.pool, project_id, "sourcemap", &hash, prefix)
        .await
    {
        Ok(()) => render_sourcemaps(&state, project_id, raw_key, None, &csrf).await,
        Err(e) => {
            render_sourcemaps(
                &state,
                project_id,
                String::new(),
                Some(format!("Error: {e}")),
                &csrf,
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
    csrf: &str,
) -> axum::response::Response {
    let existing = queries::api_keys::get_api_key_for_project(&state.pool, project_id, "sourcemap")
        .await
        .unwrap_or(None);

    let nav = queries::projects::get_nav_counts(&state.pool, project_id).await;

    let sentry_url = state.config.server.dsn_base();

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
        csrf_token: csrf.to_string(),
    };

    render_template(&tmpl)
}
