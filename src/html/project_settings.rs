use std::collections::HashMap;

use askama::Template;
use axum::extract::{Extension, Form, Path, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use stackpit_auth::AuthContext;

use crate::config::{Category, OverrideError, Resolved};
use crate::extractors::ProjectPath;
use crate::forge;
use crate::html::chrome::PageChrome;
use crate::html::flash::Flash;
use crate::html::render_template;
use crate::html::utils::{self, Chrome};
use crate::orgs::extractor::{
    require_project_owner, require_project_scope, ActiveOrg, ProjectScope,
};
use crate::orgs::{Role, SYSTEM_ORG_ID};
use crate::queries;
use crate::queries::types::{ProjectKey, ProjectRepo};
use crate::queries::users;
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

// askama needs these filters in scope for template derivation
#[allow(unused_imports)]
use crate::html::filters;

// General settings tab

/// One eligible destination org for the "move project" control.
struct MoveTarget {
    org_id: i64,
    name: String,
}

#[derive(Template)]
#[template(path = "project_settings.html")]
struct ProjectSettingsTemplate {
    project_id: u64,
    project_name: String,
    project_status: String,
    project_source: String,
    repos: Vec<ProjectRepo>,
    /// `(tag, label)` for the forge-override selector, shared by the add form
    /// and every row's edit form so the two lists cannot drift.
    forge_options: &'static [(&'static str, &'static str)],
    move_targets: Vec<MoveTarget>,
    /// `None` for members, who may not change retention.
    retention: Option<RetentionView>,
    message: Option<Flash>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

struct RetentionView {
    rows: Vec<RetentionRow>,
}

struct RetentionRow {
    /// Form field name and input id suffix.
    field: &'static str,
    label: String,
    /// `None` means forever.
    server_days: Option<u32>,
    override_days: Option<u32>,
    /// The server value has dropped to or below the stored override.
    stale: bool,
    /// Nothing is shorter than a one-day server value.
    disabled: bool,
}

fn retention_view(
    resolved: &Resolved,
    overrides: &HashMap<Category, u32>,
    chrome: &PageChrome,
) -> RetentionView {
    let rows = Category::OVERRIDABLE
        .into_iter()
        .map(|cat| {
            let server_days = resolved.days(cat);
            let override_days = overrides.get(&cat).copied();
            RetentionRow {
                field: cat.as_str(),
                label: chrome.t(&format!("projects-retention-cat-{}", cat.as_str())),
                server_days,
                override_days,
                stale: matches!((override_days, server_days), (Some(o), Some(s)) if o >= s),
                disabled: server_days == Some(1),
            }
        })
        .collect();
    RetentionView { rows }
}

const FORGE_OPTIONS: &[(&str, &str)] = &[
    ("github", "GitHub"),
    ("gitlab", "GitLab"),
    ("gitea", "Gitea / Forgejo"),
    ("bitbucket", "Bitbucket"),
    ("sourcehut", "Sourcehut"),
    ("gitee", "Gitee"),
    ("azure", "Azure DevOps"),
];

pub async fn handler(
    State(state): State<AppState>,
    active: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
) -> axum::response::Response {
    let scope = match require_project_scope(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    render_general(&state, &scope, opt_auth.as_ref(), project_id, None, &chrome).await
}

/// Resolve the calling user's id from the OIDC identity, or `None` for admin-token / anonymous callers.
async fn resolve_user_id(
    state: &AppState,
    opt_auth: Option<&Extension<AuthContext>>,
) -> Option<i64> {
    let (iss, sub) = match opt_auth.map(|e| &e.0) {
        Some(AuthContext::User { iss, sub, .. }) => (iss.as_str(), sub.as_str()),
        _ => return None,
    };
    users::find_by_iss_sub(&state.pool, iss, sub)
        .await
        .ok()
        .flatten()
        .map(|u| u.user_id)
}

/// Superuser destinations: every real (non-system, non-personal) org bar the active one.
fn superuser_move_targets(
    orgs: Vec<queries::orgs::OrgListItem>,
    active_org: i64,
) -> Vec<MoveTarget> {
    orgs.into_iter()
        .filter(|o| o.org_id != SYSTEM_ORG_ID && o.org_id != active_org && !o.is_personal)
        .map(|o| MoveTarget {
            org_id: o.org_id,
            name: o.name.unwrap_or(o.slug),
        })
        .collect()
}

/// Owner destinations: other orgs where the caller is also an owner (non-system, non-personal).
fn owner_move_targets(
    memberships: Vec<queries::orgs::Membership>,
    active_org: i64,
) -> Vec<MoveTarget> {
    memberships
        .into_iter()
        .filter(|m| {
            m.role == "owner"
                && m.org_id != active_org
                && !m.is_personal
                && m.org_id != SYSTEM_ORG_ID
        })
        .map(|m| MoveTarget {
            org_id: m.org_id,
            name: m.name.unwrap_or(m.slug),
        })
        .collect()
}

/// Orgs the caller may move this project into. Members get nothing (only owners
/// and superusers may move); the split mirrors `require_owner`. Both the role and
/// the excluded "current" org come from the project's own org, not the session's.
async fn move_targets_for(
    state: &AppState,
    scope: &ProjectScope,
    opt_auth: Option<&Extension<AuthContext>>,
) -> Vec<MoveTarget> {
    match scope.role {
        Some(Role::Member) => Vec::new(),
        None => superuser_move_targets(
            queries::orgs::list_all_orgs(&state.pool)
                .await
                .unwrap_or_default(),
            scope.org_id,
        ),
        Some(Role::Owner) => {
            let Some(user_id) = resolve_user_id(state, opt_auth).await else {
                return Vec::new();
            };
            owner_move_targets(
                queries::orgs::list_memberships(&state.pool, user_id)
                    .await
                    .unwrap_or_default(),
                scope.org_id,
            )
        }
    }
}

#[derive(Deserialize)]
pub struct SetNameForm {
    pub name: String,
}

const MAX_FIELD_LENGTH: usize = 255;

pub async fn set_name(
    State(state): State<AppState>,
    active: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
    Form(form): Form<SetNameForm>,
) -> axum::response::Response {
    let scope = match require_project_owner(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };

    let name = form.name.trim().to_string();
    if name.len() > MAX_FIELD_LENGTH {
        return render_general(
            &state,
            &scope,
            opt_auth.as_ref(),
            project_id,
            Some(Flash::err(chrome.tv1(
                "flash-project-name-too-long",
                "max",
                &MAX_FIELD_LENGTH.to_string(),
            ))),
            &chrome,
        )
        .await;
    }

    let s = state.clone();
    let success = Flash::ok(chrome.t("flash-project-name-updated"));
    let render_chrome = chrome.clone();
    let render_active = scope.clone();
    let render_auth = opt_auth.clone();
    utils::query_then_render(
        queries::projects::set_project_name(&state.writer_pool, project_id, &name).await,
        &chrome,
        success,
        move |msg| async move {
            render_general(
                &s,
                &render_active,
                render_auth.as_ref(),
                project_id,
                msg,
                &render_chrome,
            )
            .await
        },
    )
    .await
}

#[derive(Deserialize)]
pub struct AddRepoForm {
    pub repo_url: String,
    pub url_template: Option<String>,
    /// Empty means "keep the detected guess"; the select's first option.
    pub forge_type_override: Option<String>,
    pub stack_path_prefix: Option<String>,
}

pub async fn add_repo(
    State(state): State<AppState>,
    active: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
    Form(form): Form<AddRepoForm>,
) -> axum::response::Response {
    let scope = match require_project_owner(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };

    let repo_url = form.repo_url.trim().to_string();
    if repo_url.is_empty() {
        return render_general(
            &state,
            &scope,
            opt_auth.as_ref(),
            project_id,
            Some(Flash::err(chrome.t("flash-repo-url-required"))),
            &chrome,
        )
        .await;
    }
    if repo_url.len() > 2048 {
        return render_general(
            &state,
            &scope,
            opt_auth.as_ref(),
            project_id,
            Some(Flash::err(chrome.t("flash-repo-url-too-long"))),
            &chrome,
        )
        .await;
    }

    let (forge_type, _) = forge::detect_forge(&repo_url);
    let trimmed = |v: Option<String>| {
        v.map(|s| s.trim().to_string())
            .filter(|s: &String| !s.is_empty())
    };
    let url_template = trimmed(form.url_template);
    let stack_path_prefix = trimmed(form.stack_path_prefix);
    // Round-trip through the enum so an unrecognised tag can't reach the row.
    let forge_type_override = trimmed(form.forge_type_override)
        .map(|s| forge::ForgeType::from_tag(&s))
        .filter(|f| *f != forge::ForgeType::Unknown)
        .map(|f| f.as_str().to_string());

    let s = state.clone();
    let success = Flash::ok(chrome.t("flash-repo-added"));
    let render_chrome = chrome.clone();
    let render_active = scope.clone();
    let render_auth = opt_auth.clone();
    utils::query_then_render(
        queries::projects::upsert_project_repo(
            &state.writer_pool,
            project_id,
            &repo_url,
            forge_type.as_str(),
            forge_type_override.as_deref(),
            url_template.as_deref(),
            stack_path_prefix.as_deref(),
        )
        .await,
        &chrome,
        success,
        move |msg| async move {
            render_general(
                &s,
                &render_active,
                render_auth.as_ref(),
                project_id,
                msg,
                &render_chrome,
            )
            .await
        },
    )
    .await
}

pub async fn delete_repo(
    State(state): State<AppState>,
    active: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Chrome(chrome): Chrome,
    Path((project_id, repo_id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let scope = match require_project_owner(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };

    let msg = match queries::projects::delete_project_repo(&state.writer_pool, project_id, repo_id)
        .await
    {
        Ok(0) => Flash::err(format!(
            "{} {}",
            chrome.t("common-error-prefix"),
            chrome.tv1("flash-not-found-repo", "id", &repo_id.to_string())
        )),
        Ok(_) => Flash::ok(chrome.t("flash-repo-removed")),
        Err(e) => chrome.flash_err(e),
    };
    render_general(
        &state,
        &scope,
        opt_auth.as_ref(),
        project_id,
        Some(msg),
        &chrome,
    )
    .await
}

pub async fn archive_project(
    State(state): State<AppState>,
    active: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
) -> axum::response::Response {
    let scope = match require_project_owner(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };

    match queries::projects::archive_project(&state.writer_pool, project_id).await {
        Ok(0) => {
            render_general(
                &state,
                &scope,
                opt_auth.as_ref(),
                project_id,
                Some(Flash::err(format!(
                    "{} {}",
                    chrome.t("common-error-prefix"),
                    chrome.tv1("flash-not-found-project", "id", &project_id.to_string())
                ))),
                &chrome,
            )
            .await
        }
        Ok(_) => {
            // Flush the auth cache or ingestion keeps working until the entry expires.
            crate::ingest::auth::invalidate_project(
                &state.auth_cache,
                &state.negative_auth_cache,
                project_id,
            );
            render_general(
                &state,
                &scope,
                opt_auth.as_ref(),
                project_id,
                Some(Flash::ok(chrome.t("flash-project-archived"))),
                &chrome,
            )
            .await
        }
        Err(e) => {
            render_general(
                &state,
                &scope,
                opt_auth.as_ref(),
                project_id,
                Some(chrome.flash_err(e)),
                &chrome,
            )
            .await
        }
    }
}

pub async fn unarchive_project(
    State(state): State<AppState>,
    active: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
) -> axum::response::Response {
    let scope = match require_project_owner(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };

    let msg = match queries::projects::unarchive_project(&state.writer_pool, project_id).await {
        Ok(0) => Flash::err(format!(
            "{} {}",
            chrome.t("common-error-prefix"),
            chrome.tv1("flash-not-found-project", "id", &project_id.to_string())
        )),
        Ok(_) => {
            // Drop the negative Archived denials or valid keys stay rejected until TTL.
            crate::ingest::auth::invalidate_project(
                &state.auth_cache,
                &state.negative_auth_cache,
                project_id,
            );
            Flash::ok(chrome.t("flash-project-unarchived"))
        }
        Err(e) => chrome.flash_err(e),
    };
    render_general(
        &state,
        &scope,
        opt_auth.as_ref(),
        project_id,
        Some(msg),
        &chrome,
    )
    .await
}

pub async fn delete_project(
    State(state): State<AppState>,
    active: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
) -> axum::response::Response {
    let scope = match require_project_owner(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };

    match queries::projects::delete_project(&state.writer_pool, project_id).await {
        Ok(()) => {
            // Flush the auth cache or ingestion keeps working until the entry expires.
            crate::ingest::auth::invalidate_project(
                &state.auth_cache,
                &state.negative_auth_cache,
                project_id,
            );
            axum::response::Redirect::to("/web/projects/").into_response()
        }
        Err(e) => {
            render_general(
                &state,
                &scope,
                opt_auth.as_ref(),
                project_id,
                Some(chrome.flash_err(e)),
                &chrome,
            )
            .await
        }
    }
}

#[derive(Deserialize)]
pub struct MoveForm {
    pub org_id: i64,
    pub confirm_name: String,
}

pub async fn move_project(
    State(state): State<AppState>,
    active: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
    Form(form): Form<MoveForm>,
) -> axum::response::Response {
    let scope = match require_project_owner(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };

    let render_err = |msg: String| {
        render_general(
            &state,
            &scope,
            opt_auth.as_ref(),
            project_id,
            Some(Flash::err(msg)),
            &chrome,
        )
    };

    let Some(info) = queries::projects::get_project_info(&state.pool, project_id)
        .await
        .ok()
        .flatten()
    else {
        return render_err(format!(
            "{} {}",
            chrome.t("common-error-prefix"),
            chrome.tv1("flash-not-found-project", "id", &project_id.to_string())
        ))
        .await;
    };
    let project_name = info
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| format!("Project {project_id}"));

    if form.org_id == SYSTEM_ORG_ID || form.org_id == scope.org_id {
        return render_err(chrome.t("projects-move-err-invalid-target")).await;
    }

    if form.confirm_name.trim() != project_name {
        return render_err(chrome.t("projects-move-err-name-mismatch")).await;
    }

    // Target validation + authorization. For an owner, "doesn't exist", "is
    // personal", and "not owned by me" collapse to one message so a non-owner
    // can't probe which orgs exist; a superuser sees the plain invalid-target.
    let actor_user_id = resolve_user_id(&state, opt_auth.as_ref()).await;
    let target_ok = if scope.role.is_none() {
        matches!(
            queries::orgs::get_org(&state.pool, form.org_id).await,
            Ok(Some(o)) if !o.is_personal
        )
    } else {
        let Some(user_id) = actor_user_id else {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        };
        let non_personal = matches!(
            queries::orgs::get_org(&state.pool, form.org_id).await,
            Ok(Some(o)) if !o.is_personal
        );
        let owns = queries::orgs::is_current_owner(&state.pool, user_id, form.org_id)
            .await
            .unwrap_or(false);
        non_personal && owns
    };

    if !target_ok {
        let key = if scope.role.is_none() {
            "projects-move-err-invalid-target"
        } else {
            "projects-move-err-denied"
        };
        return render_err(chrome.t(key)).await;
    }

    match queries::projects::move_project_to_org(
        &state.writer_pool,
        project_id as i64,
        scope.org_id,
        form.org_id,
    )
    .await
    {
        Ok(true) => {
            tracing::info!(
                target: "stackpit::audit",
                actor_user_id = ?actor_user_id,
                project_id,
                from_org = scope.org_id,
                to_org = form.org_id,
                "project moved to another organization"
            );
            axum::response::Redirect::to("/web/projects/").into_response()
        }
        Ok(false) => render_err(chrome.t("projects-move-err-conflict")).await,
        Err(e) => render_err(chrome.err(e)).await,
    }
}

/// Absent field: unchanged (disabled inputs are not submitted). Blank: clear.
#[derive(Deserialize)]
pub struct RetentionForm {
    pub errors: Option<String>,
    pub traces: Option<String>,
    pub logs: Option<String>,
    pub metrics: Option<String>,
}

impl RetentionForm {
    fn field(&self, cat: Category) -> Option<&str> {
        match cat {
            Category::Errors => self.errors.as_deref(),
            Category::Traces => self.traces.as_deref(),
            Category::Logs => self.logs.as_deref(),
            Category::Metrics => self.metrics.as_deref(),
            Category::Releases => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RetentionFormError {
    Invalid,
    NotShorter,
}

/// The changes the form asks for, validated against the server values. Fields
/// equal to the stored override are skipped, so a stale override survives an
/// unrelated save.
fn retention_changes(
    form: &RetentionForm,
    resolved: &Resolved,
    stored: &HashMap<Category, u32>,
) -> Result<Vec<(Category, Option<u32>)>, RetentionFormError> {
    let mut changes = Vec::new();
    for cat in Category::OVERRIDABLE {
        let Some(raw) = form.field(cat) else {
            continue;
        };
        let raw = raw.trim();
        let new = if raw.is_empty() {
            None
        } else {
            Some(
                raw.parse::<u32>()
                    .map_err(|_| RetentionFormError::Invalid)?,
            )
        };
        if new == stored.get(&cat).copied() {
            continue;
        }
        if let Some(days) = new {
            resolved.check_override(cat, days).map_err(|e| match e {
                OverrideError::NotShorter => RetentionFormError::NotShorter,
                OverrideError::Zero | OverrideError::TooLarge | OverrideError::NotOverridable => {
                    RetentionFormError::Invalid
                }
            })?;
        }
        changes.push((cat, new));
    }
    Ok(changes)
}

pub async fn set_retention(
    State(state): State<AppState>,
    active: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
    Form(form): Form<RetentionForm>,
) -> axum::response::Response {
    let scope = match require_project_owner(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };

    let resolved = state.config.storage.resolved_retention();
    let msg = match queries::retention::get_overrides(&state.writer_pool, project_id as i64).await {
        Err(e) => chrome.flash_err(e),
        Ok(stored) => match retention_changes(&form, &resolved, &stored) {
            Err(RetentionFormError::Invalid) => Flash::err(chrome.tv1(
                "flash-retention-invalid",
                "max",
                &crate::config::MAX_OVERRIDE_DAYS.to_string(),
            )),
            Err(RetentionFormError::NotShorter) => {
                Flash::err(chrome.t("flash-retention-not-shorter"))
            }
            Ok(changes) if changes.is_empty() => Flash::ok(chrome.t("flash-retention-saved")),
            Ok(changes) => match queries::retention::apply_overrides(
                &state.writer_pool,
                project_id as i64,
                &changes,
            )
            .await
            {
                Ok(()) => Flash::ok(chrome.t("flash-retention-saved")),
                Err(e) => chrome.flash_err(e),
            },
        },
    };
    render_general(
        &state,
        &scope,
        opt_auth.as_ref(),
        project_id,
        Some(msg),
        &chrome,
    )
    .await
}

async fn render_general(
    state: &AppState,
    scope: &ProjectScope,
    opt_auth: Option<&Extension<AuthContext>>,
    project_id: u64,
    message: Option<Flash>,
    chrome: &PageChrome,
) -> axum::response::Response {
    let repos = queries::projects::get_project_repos(&state.pool, project_id)
        .await
        .unwrap_or_default();
    let move_targets = move_targets_for(state, scope, opt_auth).await;
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

    let retention = if scope.role == Some(Role::Member) {
        None
    } else {
        let overrides = queries::retention::get_overrides(&state.pool, project_id as i64)
            .await
            .unwrap_or_default();
        Some(retention_view(
            &state.config.storage.resolved_retention(),
            &overrides,
            chrome,
        ))
    };

    let nav = state.nav_counts(project_id).await;

    let tmpl = ProjectSettingsTemplate {
        project_id,
        project_name,
        project_status,
        project_source,
        repos,
        forge_options: FORGE_OPTIONS,
        move_targets,
        retention,
        message,
        nav,
        chrome: chrome.clone(),
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
    message: Option<Flash>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn keys_handler(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    render_keys(&state, project_id, None, &chrome).await
}

#[derive(Deserialize)]
pub struct CreateKeyForm {
    pub label: Option<String>,
}

pub async fn create_key(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
    Form(form): Form<CreateKeyForm>,
) -> axum::response::Response {
    if let Err(r) = require_project_owner(&active, &state.pool, project_id as i64).await {
        return r;
    }

    let label = form
        .label
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());

    let s = state.clone();
    let success = Flash::ok(chrome.t("flash-key-created"));
    let render_chrome = chrome.clone();
    utils::query_then_render(
        queries::projects::create_project_key(&state.writer_pool, project_id, label.as_deref())
            .await,
        &chrome,
        success,
        move |msg| async move { render_keys(&s, project_id, msg, &render_chrome).await },
    )
    .await
}

pub async fn delete_key(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    Path((project_id, public_key)): Path<(u64, String)>,
) -> axum::response::Response {
    if let Err(r) = require_project_owner(&active, &state.pool, project_id as i64).await {
        return r;
    }

    match queries::projects::delete_project_key(&state.writer_pool, project_id, &public_key).await {
        Ok(0) => {
            render_keys(
                &state,
                project_id,
                Some(Flash::err(format!(
                    "{} {}",
                    chrome.t("common-error-prefix"),
                    chrome.tv1("flash-not-found-key", "id", &public_key)
                ))),
                &chrome,
            )
            .await
        }
        Ok(_) => {
            crate::ingest::auth::invalidate_key(
                &state.auth_cache,
                &state.negative_auth_cache,
                &public_key,
            );
            render_keys(
                &state,
                project_id,
                Some(Flash::ok(chrome.t("flash-key-deleted"))),
                &chrome,
            )
            .await
        }
        Err(e) => render_keys(&state, project_id, Some(chrome.flash_err(e)), &chrome).await,
    }
}

async fn render_keys(
    state: &AppState,
    project_id: u64,
    message: Option<Flash>,
    chrome: &PageChrome,
) -> axum::response::Response {
    let keys = queries::projects::list_project_keys(&state.pool, project_id)
        .await
        .unwrap_or_default();

    let nav = state.nav_counts(project_id).await;

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
        chrome: chrome.clone(),
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
    message: Option<Flash>,
    sentry_url: String,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn sourcemaps_handler(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    render_sourcemaps(&state, project_id, String::new(), None, &chrome).await
}

pub async fn generate_sourcemap_key(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
) -> axum::response::Response {
    if let Err(r) = require_project_owner(&active, &state.pool, project_id as i64).await {
        return r;
    }

    let raw_key = format!("spk_{}", crate::util::crypto::random_hex::<16>());

    let hash = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(raw_key.as_bytes()))
    };

    let prefix = &raw_key[..12];

    match queries::api_keys::create_api_key(
        &state.writer_pool,
        project_id,
        "sourcemap",
        &hash,
        prefix,
    )
    .await
    {
        Ok(()) => render_sourcemaps(&state, project_id, raw_key, None, &chrome).await,
        Err(e) => {
            render_sourcemaps(
                &state,
                project_id,
                String::new(),
                Some(chrome.flash_err(e)),
                &chrome,
            )
            .await
        }
    }
}

async fn render_sourcemaps(
    state: &AppState,
    project_id: u64,
    new_key: String,
    message: Option<Flash>,
    chrome: &PageChrome,
) -> axum::response::Response {
    let existing = queries::api_keys::get_api_key_for_project(&state.pool, project_id, "sourcemap")
        .await
        .unwrap_or(None);

    let nav = state.nav_counts(project_id).await;

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
        chrome: chrome.clone(),
    };

    render_template(&tmpl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::orgs::{Membership, OrgListItem};

    fn org(id: i64, personal: bool) -> OrgListItem {
        OrgListItem {
            org_id: id,
            slug: format!("org-{id}"),
            name: Some(format!("Org {id}")),
            is_personal: personal,
            ext_iss: None,
        }
    }

    fn mem(id: i64, role: &str, personal: bool) -> Membership {
        Membership {
            org_id: id,
            role: role.to_string(),
            slug: format!("org-{id}"),
            name: Some(format!("Org {id}")),
            is_personal: personal,
            ext_iss: None,
            ext_org_id: None,
        }
    }

    fn ids(targets: &[MoveTarget]) -> Vec<i64> {
        targets.iter().map(|t| t.org_id).collect()
    }

    #[test]
    fn superuser_targets_exclude_system_personal_and_active() {
        let orgs = vec![
            org(SYSTEM_ORG_ID, false),
            org(5, false),
            org(6, true),
            org(7, false),
        ];
        // active = 7; system(1), personal(6), and active(7) drop out.
        assert_eq!(ids(&superuser_move_targets(orgs, 7)), vec![5]);
    }

    #[test]
    fn owner_targets_only_owned_non_personal_non_active() {
        let ms = vec![
            mem(10, "owner", false),            // active -> excluded
            mem(11, "owner", false),            // kept
            mem(12, "member", false),           // not an owner -> excluded
            mem(13, "owner", true),             // personal -> excluded
            mem(SYSTEM_ORG_ID, "owner", false), // system -> excluded
        ];
        assert_eq!(ids(&owner_move_targets(ms, 10)), vec![11]);
    }

    fn repo(forge: &str, over: Option<&str>, prefix: Option<&str>) -> ProjectRepo {
        ProjectRepo {
            id: 1,
            project_id: 7,
            repo_url: "https://git.gofranz.com/franz/stackpit".into(),
            forge_type: forge.into(),
            forge_type_override: over.map(String::from),
            url_template: None,
            stack_path_prefix: prefix.map(String::from),
        }
    }

    #[test]
    fn project_settings_renders_with_and_without_the_new_repo_columns() {
        use askama::Template;
        use unic_langid::langid;

        for locale in [langid!("en"), langid!("de")] {
            let chrome = PageChrome::new("csrf".into(), locale.clone(), "/web/projects/".into());
            for repos in [
                Vec::new(),
                vec![repo("unknown", Some("gitea"), Some("services/api/"))],
                vec![repo("github", None, None)],
            ] {
                let html = ProjectSettingsTemplate {
                    project_id: 7,
                    project_name: "demo".into(),
                    project_status: "active".into(),
                    project_source: "manual".into(),
                    repos,
                    forge_options: FORGE_OPTIONS,
                    move_targets: Vec::new(),
                    retention: Some(retention_view(
                        &Resolved::uniform(90),
                        &HashMap::from([(Category::Traces, 120)]),
                        &chrome,
                    )),
                    message: None,
                    nav: ProjectNavCounts::default(),
                    chrome: chrome.clone(),
                }
                .render()
                .expect("project settings renders");

                assert!(
                    !html.contains("Unknown localization"),
                    "missing Fluent key in {locale}"
                );
                assert!(html.contains("stack_path_prefix"));
                assert!(html.contains("forge_type_override"));
                assert!(html.contains(r#"name="traces""#));
            }
        }
    }

    #[test]
    fn settings_table_shows_the_override_not_the_detected_forge() {
        use askama::Template;
        use unic_langid::langid;

        let chrome = PageChrome::new("csrf".into(), langid!("en"), "/web/projects/".into());
        let html = ProjectSettingsTemplate {
            project_id: 7,
            project_name: "demo".into(),
            project_status: "active".into(),
            project_source: "manual".into(),
            repos: vec![repo("unknown", Some("gitea"), None)],
            forge_options: FORGE_OPTIONS,
            move_targets: Vec::new(),
            retention: None,
            message: None,
            nav: ProjectNavCounts::default(),
            chrome,
        }
        .render()
        .expect("project settings renders");

        assert!(html.contains(">gitea<"));
        assert!(!html.contains(">unknown<"));
    }

    /// `upsert_project_repo` keys on `repo_url`, so an editable URL field would
    /// silently create a second row and orphan the one being edited.
    #[test]
    fn the_inline_edit_form_carries_the_url_but_never_as_an_input() {
        use askama::Template;
        use unic_langid::langid;

        let chrome = PageChrome::new("csrf".into(), langid!("en"), "/web/projects/".into());
        let html = ProjectSettingsTemplate {
            project_id: 7,
            project_name: "demo".into(),
            project_status: "active".into(),
            project_source: "manual".into(),
            repos: vec![repo("unknown", None, None)],
            forge_options: FORGE_OPTIONS,
            move_targets: Vec::new(),
            retention: None,
            message: None,
            nav: ProjectNavCounts::default(),
            chrome,
        }
        .render()
        .expect("project settings renders");

        // One `repo_url` per rendered row, plus the add form's own field.
        assert_eq!(html.matches(r#"name="repo_url""#).count(), 2);
        assert_eq!(
            html.matches(r#"<input type="hidden" name="repo_url""#)
                .count(),
            1,
            "the edit form submits the URL it was rendered from, as a hidden field"
        );
        // The one editable `repo_url` belongs to the add form, which is the
        // only place a new row is meant to appear.
        assert_eq!(
            html.matches(r#"id="repo_url""#).count(),
            1,
            "exactly one editable URL field, and it is the add form's"
        );
        // The inert row says so, and points at the control that fixes it.
        assert!(html.contains("forge unknown"));
    }

    use crate::orgs::extractor::ActiveOrg;
    use axum::http::StatusCode;

    const ORG: i64 = 5;
    const PROJECT: u64 = 7;

    async fn seeded_pool() -> crate::db::DbPool {
        let pool = crate::db::open_test_pool().await;
        sqlx::query(crate::db::sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (5, 'acme', 'Acme')"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(crate::db::sql!(
            "INSERT INTO projects (project_id, status, source, org_id) \
             VALUES (7, 'active', 'manual', 5)"
        ))
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    /// Server values: errors 30, traces 90, logs 90, metrics forever.
    fn state_for(pool: &crate::db::DbPool) -> AppState {
        let (mut state, _chans) = AppState::for_test(pool.clone());
        let mut config = crate::config::Config::default();
        config.storage.retention_days = 90;
        config.storage.retention.errors = Some(30);
        config.storage.retention.metrics = Some(0);
        state.config = std::sync::Arc::new(config);
        state
    }

    fn as_role(role: Option<Role>) -> ActiveOrg {
        match role {
            Some(r) => ActiveOrg {
                session_org_id: ORG,
                role: Some(r),
                org_name: None,
                memberships: vec![(ORG, r)],
            },
            None => ActiveOrg::bare(ORG, None),
        }
    }

    fn page_chrome() -> Chrome {
        use unic_langid::langid;
        Chrome(PageChrome::new(
            "csrf".into(),
            langid!("en"),
            "/web/projects/".into(),
        ))
    }

    fn form(fields: &[(Category, &str)]) -> RetentionForm {
        let get = |cat| {
            fields
                .iter()
                .find(|(c, _)| *c == cat)
                .map(|(_, v)| (*v).to_string())
        };
        RetentionForm {
            errors: get(Category::Errors),
            traces: get(Category::Traces),
            logs: get(Category::Logs),
            metrics: get(Category::Metrics),
        }
    }

    async fn post(
        state: &AppState,
        role: Option<Role>,
        fields: &[(Category, &str)],
    ) -> (StatusCode, String) {
        let resp = set_retention(
            State(state.clone()),
            as_role(role),
            None,
            page_chrome(),
            ProjectPath(PROJECT),
            Form(form(fields)),
        )
        .await;
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    async fn stored(pool: &crate::db::DbPool) -> HashMap<Category, u32> {
        queries::retention::get_overrides(pool, PROJECT as i64)
            .await
            .unwrap()
    }

    const SAVED: &str = "Retention settings saved";
    const NOT_SHORTER: &str = "A project override must be shorter than the server retention";
    const INVALID: &str = "Retention must be a whole number of days";

    #[tokio::test]
    async fn retention_owner_saves_and_the_page_shows_it() {
        let pool = seeded_pool().await;
        let state = state_for(&pool);

        let (status, body) = post(
            &state,
            Some(Role::Owner),
            &[
                (Category::Errors, ""),
                (Category::Traces, " 14 "),
                (Category::Logs, ""),
                (Category::Metrics, "400"),
            ],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains(SAVED), "{body}");
        assert_eq!(
            stored(&pool).await,
            HashMap::from([(Category::Traces, 14), (Category::Metrics, 400)])
        );
        assert!(body.contains(r#"id="retention_traces""#));
        assert!(body.contains(r#"value="14""#));
    }

    #[tokio::test]
    async fn retention_value_not_below_server_writes_nothing() {
        let pool = seeded_pool().await;
        let state = state_for(&pool);

        for days in ["90", "91"] {
            let (status, body) = post(&state, Some(Role::Owner), &[(Category::Traces, days)]).await;
            assert_eq!(status, StatusCode::OK);
            assert!(body.contains(NOT_SHORTER), "{days}: {body}");
        }
        for bad in ["0", "-3", "1.5", "abc", "36501", "99999999999"] {
            let (_, body) = post(&state, Some(Role::Owner), &[(Category::Metrics, bad)]).await;
            assert!(body.contains(INVALID), "{bad}: {body}");
        }
        assert!(stored(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn retention_blank_clears() {
        let pool = seeded_pool().await;
        queries::retention::apply_overrides(
            &pool,
            PROJECT as i64,
            &[(Category::Traces, Some(7)), (Category::Logs, Some(7))],
        )
        .await
        .unwrap();
        let state = state_for(&pool);

        let (_, body) = post(
            &state,
            Some(Role::Owner),
            &[(Category::Traces, "  "), (Category::Logs, "7")],
        )
        .await;
        assert!(body.contains(SAVED), "{body}");
        assert_eq!(stored(&pool).await, HashMap::from([(Category::Logs, 7)]));
    }

    #[tokio::test]
    async fn retention_mixed_valid_and_invalid_writes_nothing() {
        let pool = seeded_pool().await;
        queries::retention::apply_overrides(&pool, PROJECT as i64, &[(Category::Logs, Some(7))])
            .await
            .unwrap();
        let state = state_for(&pool);

        let (_, body) = post(
            &state,
            Some(Role::Owner),
            &[
                (Category::Errors, "10"),
                (Category::Traces, "120"),
                (Category::Logs, ""),
            ],
        )
        .await;
        assert!(body.contains(NOT_SHORTER), "{body}");
        assert_eq!(stored(&pool).await, HashMap::from([(Category::Logs, 7)]));
    }

    #[tokio::test]
    async fn retention_stale_override_survives_an_unrelated_change() {
        let pool = seeded_pool().await;
        queries::retention::apply_overrides(&pool, PROJECT as i64, &[(Category::Errors, Some(60))])
            .await
            .unwrap();
        let state = state_for(&pool);

        let (_, body) = post(
            &state,
            Some(Role::Owner),
            &[(Category::Errors, "60"), (Category::Logs, "10")],
        )
        .await;
        assert!(body.contains(SAVED), "{body}");
        assert_eq!(
            stored(&pool).await,
            HashMap::from([(Category::Errors, 60), (Category::Logs, 10)])
        );
        assert!(body.contains("No longer effective"), "{body}");
    }

    #[tokio::test]
    async fn retention_member_is_refused_and_sees_no_section() {
        let pool = seeded_pool().await;
        let state = state_for(&pool);

        let (status, _) = post(&state, Some(Role::Member), &[(Category::Traces, "7")]).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(stored(&pool).await.is_empty());

        for (role, shown) in [
            (Some(Role::Member), false),
            (Some(Role::Owner), true),
            (None, true),
        ] {
            let resp = handler(
                State(state.clone()),
                as_role(role),
                None,
                page_chrome(),
                ProjectPath(PROJECT),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::OK);
            let body = axum::body::to_bytes(resp.into_body(), 4 * 1024 * 1024)
                .await
                .unwrap();
            let body = String::from_utf8_lossy(&body);
            assert_eq!(body.contains("/settings/retention"), shown, "role {role:?}");
        }
    }

    #[tokio::test]
    async fn retention_superuser_saves() {
        let pool = seeded_pool().await;
        let state = state_for(&pool);

        let (status, body) = post(&state, None, &[(Category::Traces, "7")]).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains(SAVED), "{body}");
        assert_eq!(stored(&pool).await, HashMap::from([(Category::Traces, 7)]));
    }

    #[test]
    fn retention_view_flags_stale_and_one_day_server_values() {
        use unic_langid::langid;
        let chrome = PageChrome::new("csrf".into(), langid!("en"), "/web/projects/".into());
        let resolved = Resolved {
            base: Some(90),
            errors: Some(30),
            traces: Some(1),
            logs: Some(90),
            metrics: None,
            releases: Some(90),
        };
        let overrides = HashMap::from([
            (Category::Errors, 30),
            (Category::Logs, 10),
            (Category::Metrics, 500),
        ]);
        let view = retention_view(&resolved, &overrides, &chrome);
        let fields: Vec<_> = view.rows.iter().map(|r| r.field).collect();
        assert_eq!(fields, ["errors", "traces", "logs", "metrics"]);
        let flags: Vec<_> = view.rows.iter().map(|r| (r.stale, r.disabled)).collect();
        assert_eq!(
            flags,
            [(true, false), (false, true), (false, false), (false, false)]
        );
        assert_eq!(view.rows[0].label, "Errors");
    }
}
