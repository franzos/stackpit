use askama::Template;
use axum::extract::{Form, Path, State};
use serde::Deserialize;

use crate::extractors::ProjectPath;
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

/// Build the per-project recipient config JSON, validating a non-empty value is
/// a real email address. Returns `Err(flash_key)` on an invalid address so the
/// caller can surface it rather than store junk that only fails at send time.
fn recipient_config(to_address: Option<String>) -> Result<Option<String>, &'static str> {
    match to_address
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(addr) if !email_address::EmailAddress::is_valid(addr) => {
            Err("flash-invalid-to-address")
        }
        Some(addr) => Ok(Some(serde_json::json!({ "to": addr }).to_string())),
        None => Ok(None),
    }
}

/// Pre-filled tracker override fields for one integration, parsed from the
/// `project_tracker_targets.target` JSON blob.
struct TrackerOverrideView {
    owner: Option<String>,
    repo: Option<String>,
    tracker_project_id: Option<i64>,
}

impl TrackerOverrideView {
    fn from_target(target: &serde_json::Value) -> Self {
        Self {
            owner: target
                .get("owner")
                .and_then(|v| v.as_str())
                .map(String::from),
            repo: target
                .get("repo")
                .and_then(|v| v.as_str())
                .map(String::from),
            tracker_project_id: target.get("project_id").and_then(|v| v.as_i64()),
        }
    }
}

#[derive(Template)]
#[template(path = "project_integrations.html")]
struct ProjectIntegrationsTemplate {
    project_id: u64,
    active: Vec<ProjectIntegration>,
    available: Vec<Integration>,
    overrides: std::collections::HashMap<i64, TrackerOverrideView>,
    message: Option<String>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn handler(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
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
    ProjectPath(project_id): ProjectPath,
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
    let config = match recipient_config(form.to_address) {
        Ok(c) => c,
        Err(key) => {
            return render_page(
                &state,
                project_id,
                Some(chrome.t(key)),
                &chrome,
                active.org_id,
            )
            .await
        }
    };

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
    let config = match recipient_config(form.to_address) {
        Ok(c) => c,
        Err(key) => {
            return render_page(
                &state,
                project_id,
                Some(chrome.t(key)),
                &chrome,
                active.org_id,
            )
            .await
        }
    };

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

/// Send a real test notification through one activated project integration.
/// Unlike the global-list test, this has the per-project recipient, so email
/// integrations actually deliver.
pub async fn test(
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

    let pis = match queries::integrations::list_project_integrations(&state.pool, project_id).await
    {
        Ok(v) => v,
        Err(e) => {
            return render_page(
                &state,
                project_id,
                Some(chrome.err(e)),
                &chrome,
                active.org_id,
            )
            .await
        }
    };
    let Some(pi) = pis.into_iter().find(|p| p.id == id) else {
        return render_page(
            &state,
            project_id,
            Some(chrome.t("flash-integration-not-found")),
            &chrome,
            active.org_id,
        )
        .await;
    };

    let secret = match (
        &pi.integration_secret,
        pi.integration_encrypted,
        &state.encryptor,
    ) {
        (Some(s), true, Some(enc)) => enc.decrypt(s),
        (Some(s), false, _) => Some(s.clone()),
        _ => None,
    };

    let event = crate::notify::NotificationEvent {
        trigger: crate::notify::NotifyTrigger::NewIssue,
        project_id,
        // Empty: a test notification has no real issue, so no (dead) link is added.
        fingerprint: String::new(),
        title: Some("Test notification from Stackpit".to_string()),
        level: Some("info".to_string()),
        environment: Some("test".to_string()),
        environments: vec!["test".to_string()],
        event_id: "test-event-id".to_string(),
        digest: None,
    };

    let result = if let crate::domain::IntegrationKind::Email = pi.integration_kind {
        // pi.config carries the per-project recipient ({"to": ...}).
        match state.config.email.as_ref() {
            Some(email_cfg) => {
                crate::providers::email::send(
                    email_cfg,
                    &state.config.server.web_base(),
                    secret.as_deref(),
                    pi.integration_config.as_deref(),
                    pi.config.as_deref(),
                    &event,
                )
                .await
            }
            None => Err(anyhow::anyhow!(
                "email is not configured ([email] section absent)"
            )),
        }
    } else {
        let url = match pi.integration_url.as_deref() {
            Some(u) if !u.is_empty() => u,
            _ => {
                return render_page(
                    &state,
                    project_id,
                    Some(chrome.t("flash-integration-no-url")),
                    &chrome,
                    active.org_id,
                )
                .await
            }
        };
        // Pin resolved DNS so reqwest can't re-resolve to an internal address.
        let resolved = match crate::util::ssrf::check_ssrf(url).await {
            Ok(r) => r,
            Err(msg) => {
                return render_page(&state, project_id, Some(msg), &chrome, active.org_id).await
            }
        };
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .resolve(&resolved.hostname, resolved.addr)
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("failed to build pinned reqwest client: {e}");
                return render_page(
                    &state,
                    project_id,
                    Some(chrome.tv1("flash-test-failed", "error", "internal error")),
                    &chrome,
                    active.org_id,
                )
                .await;
            }
        };
        crate::providers::dispatch(
            &client,
            &pi.integration_kind,
            url,
            secret.as_deref(),
            &event,
        )
        .await
    };

    let msg = match result {
        Ok(()) => chrome.t("flash-test-notification-sent"),
        Err(e) => chrome.tv1("flash-test-failed", "error", &e.to_string()),
    };
    render_page(&state, project_id, Some(msg), &chrome, active.org_id).await
}

#[derive(Deserialize)]
pub struct TargetForm {
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub project_id: Option<String>,
}

/// Sets the optional per-project tracker target override (D2): a standalone
/// row in `project_tracker_targets`, kept out of the notify `activate`/`update`
/// flow so it never surfaces in the live dispatcher or the Alerts Hub.
pub async fn set_target(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    Path((project_id, id)): Path<(u64, i64)>,
    Form(form): Form<TargetForm>,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }
    // Reject cross-org targets: the integration must belong to the active org.
    match queries::integrations::get_integration(&state.pool, id, Some(active.org_id)).await {
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

    let owner = form.owner.filter(|s| !s.trim().is_empty());
    let repo = form.repo.filter(|s| !s.trim().is_empty());
    let tracker_project_id = form
        .project_id
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| s.trim().parse::<i64>().ok());

    let mut target = serde_json::Map::new();
    if let Some(o) = owner {
        target.insert("owner".to_string(), serde_json::Value::String(o));
    }
    if let Some(r) = repo {
        target.insert("repo".to_string(), serde_json::Value::String(r));
    }
    if let Some(p) = tracker_project_id {
        target.insert("project_id".to_string(), serde_json::Value::from(p));
    }

    let result = if target.is_empty() {
        queries::tracker_targets::delete_override(&state.writer_pool, project_id as i64, id).await
    } else {
        queries::tracker_targets::set_override(
            &state.writer_pool,
            project_id as i64,
            id,
            &serde_json::Value::Object(target),
        )
        .await
    };
    let msg = match result {
        Ok(()) => chrome.t("flash-integration-target-saved"),
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

    let mut overrides = std::collections::HashMap::new();
    for pi in &active {
        if pi.integration_kind.is_tracker() {
            if let Ok(Some(target)) = queries::tracker_targets::get_override(
                &state.pool,
                project_id as i64,
                pi.integration_id,
            )
            .await
            {
                overrides.insert(pi.integration_id, TrackerOverrideView::from_target(&target));
            }
        }
    }

    let nav = state.nav_counts(project_id).await;

    let tmpl = ProjectIntegrationsTemplate {
        project_id,
        active,
        available,
        overrides,
        message,
        nav,
        chrome: chrome.clone(),
    };

    render_template(&tmpl)
}
