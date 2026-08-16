use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::domain::IntegrationKind;
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::Chrome;
use crate::orgs::extractor::{require_org_owner, ActiveOrg};
use crate::queries;
use crate::queries::types::Integration;
use crate::server::AppState;

use super::html_error;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "integrations.html")]
struct IntegrationsTemplate {
    integrations: Vec<Integration>,
    message: Option<String>,
    chrome: PageChrome,
    /// Drives the upsell banner and the locked "add" buttons. False means the
    /// gated kinds are visible but unusable, rather than silently missing.
    integrations_licensed: bool,
}

pub async fn handler(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
) -> axum::response::Response {
    let org_filter = active.role.as_ref().map(|_| active.session_org_id);
    render_list(&state, org_filter, None, &chrome).await
}

#[derive(Deserialize)]
pub struct CreateForm {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub url: String,
    pub secret: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    pub to_address: Option<String>,
    #[serde(default)]
    pub is_global: Option<String>,
}

/// Validates the default recipient - a global email integration must carry one.
fn default_recipient(
    to_address: Option<&str>,
    is_global: bool,
) -> Result<Option<String>, &'static str> {
    match to_address.map(str::trim).filter(|s| !s.is_empty()) {
        Some(addr) if !email_address::EmailAddress::is_valid(addr) => {
            Err("flash-invalid-to-address")
        }
        Some(addr) => Ok(Some(addr.to_string())),
        None if is_global => Err("flash-global-email-needs-recipient"),
        None => Ok(None),
    }
}

pub async fn create(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
    Form(form): Form<CreateForm>,
) -> axum::response::Response {
    if let Err(r) = require_org_owner(&active) {
        return r;
    }
    let org_filter = active.role.as_ref().map(|_| active.session_org_id);
    let name = form.name.trim().to_string();
    if name.is_empty() {
        return render_list(
            &state,
            org_filter,
            Some(chrome.t("flash-name-required")),
            &chrome,
        )
        .await;
    }
    let kind = form.kind.trim().to_string();
    let Ok(parsed_kind) = kind.parse::<crate::domain::IntegrationKind>() else {
        return render_list(
            &state,
            org_filter,
            Some(chrome.t("flash-invalid-integration-kind")),
            &chrome,
        )
        .await;
    };
    if !crate::commercial::providers::may_configure(&state.license, parsed_kind) {
        return render_list(
            &state,
            org_filter,
            Some(chrome.t("flash-integration-license-required")),
            &chrome,
        )
        .await;
    }
    // A tracker's projects follow from its repositories, so global means nothing.
    let is_global = form.is_global.is_some() && !parsed_kind.is_tracker();

    // Email has no user-controlled endpoint, so `url` stays NULL and there's no
    // SSRF surface. A locked mailer ignores any submitted token.
    use crate::providers::email as email_provider;
    let (url, config, ignore_secret) = if kind == "email" {
        let Some(email_cfg) = state.config.email.as_ref() else {
            return render_list(
                &state,
                org_filter,
                Some(chrome.t("flash-email-not-configured")),
                &chrome,
            )
            .await;
        };
        let default_to = match default_recipient(form.to_address.as_deref(), is_global) {
            Ok(t) => t,
            Err(key) => return render_list(&state, org_filter, Some(chrome.t(key)), &chrome).await,
        };
        if email_cfg.lock {
            let mut cfg = serde_json::json!({
                "provider": email_provider::provider_label(&email_cfg.provider),
            });
            if let Some(to) = default_to {
                cfg["to"] = serde_json::json!(to);
            }
            (None, Some(cfg.to_string()), true)
        } else {
            let provider_str = form.provider.as_deref().map(str::trim).unwrap_or("");
            if !email_provider::provider_is_known(provider_str) {
                return render_list(
                    &state,
                    org_filter,
                    Some(chrome.t("flash-invalid-email-provider")),
                    &chrome,
                )
                .await;
            }
            // Reject up front when the credential needed to send mail is missing;
            // otherwise the failure only surfaces at dispatch time. API providers
            // need a token (form or the matching instance token); SMTP has no
            // per-integration token and is only offered when the instance
            // provider is itself SMTP.
            let is_smtp = provider_str == "smtp";
            if is_smtp {
                if email_provider::provider_label(&email_cfg.provider) != "smtp" {
                    return render_list(
                        &state,
                        org_filter,
                        Some(chrome.t("flash-smtp-not-configured")),
                        &chrome,
                    )
                    .await;
                }
            } else {
                let has_form_secret = form
                    .secret
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|s| !s.is_empty());
                let has_global_token =
                    email_provider::global_api_token(&email_cfg.provider, provider_str).is_some();
                if !has_form_secret && !has_global_token {
                    return render_list(
                        &state,
                        org_filter,
                        Some(chrome.t("flash-api-token-required")),
                        &chrome,
                    )
                    .await;
                }
            }
            let has_form_from = form
                .from_address
                .as_deref()
                .map(str::trim)
                .is_some_and(|s| !s.is_empty());
            if !has_form_from && email_cfg.from_address.is_none() {
                return render_list(
                    &state,
                    org_filter,
                    Some(chrome.t("flash-from-address-required")),
                    &chrome,
                )
                .await;
            }
            let mut cfg = serde_json::json!({ "provider": provider_str });
            if let Some(from) = form
                .from_address
                .as_deref()
                .map(str::trim)
                .filter(|f| !f.is_empty())
            {
                cfg["from"] = serde_json::json!(from);
            }
            if let Some(name) = form
                .from_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
            {
                cfg["from_name"] = serde_json::json!(name);
            }
            if let Some(to) = default_to {
                cfg["to"] = serde_json::json!(to);
            }
            // SMTP has no per-integration token, so never store a submitted secret.
            (None, Some(cfg.to_string()), is_smtp)
        }
    } else if ["github", "forgejo", "gitlab"].contains(&kind.as_str()) {
        let base_url = form.url.trim().to_string();
        if base_url.is_empty() {
            return render_list(
                &state,
                org_filter,
                Some(chrome.t("flash-url-required")),
                &chrome,
            )
            .await;
        }
        // Same SSRF gate as the webhook branch: trackers point at a
        // user-controlled base URL (self-hosted Forgejo/GitLab instances).
        if let Err(msg) = crate::util::ssrf::check_ssrf(&base_url).await {
            return render_list(&state, org_filter, Some(msg), &chrome).await;
        }
        let has_token = form
            .secret
            .as_deref()
            .map(str::trim)
            .is_some_and(|s| !s.is_empty());
        if !has_token {
            return render_list(
                &state,
                org_filter,
                Some(chrome.t("flash-api-token-required")),
                &chrome,
            )
            .await;
        }
        // A tracker files into the project's own repositories, so no target to store.
        (Some(base_url), None, false)
    } else {
        let url = form.url.trim().to_string();
        if url.is_empty() {
            return render_list(
                &state,
                org_filter,
                Some(chrome.t("flash-url-required")),
                &chrome,
            )
            .await;
        }
        // Block webhooks pointing at private/internal addresses. Validation only,
        // no request here, so no TOCTOU; the dispatcher does its own pinned resolution.
        if let Err(msg) = crate::util::ssrf::check_ssrf(&url).await {
            return render_list(&state, org_filter, Some(msg), &chrome).await;
        }
        (Some(url), None, false)
    };

    let raw_secret = if ignore_secret {
        None
    } else {
        form.secret
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
    };

    // Refuse to store plaintext: secrets are encrypted or not stored.
    let (secret, encrypted) = match raw_secret {
        Some(ref s) => match crate::util::crypto::encrypt_secret(s, state.encryptor.as_deref()) {
            Ok(val) => (Some(val), true),
            Err(e) => {
                tracing::warn!("refusing to store plaintext secret: {e}");
                return render_list(
                    &state,
                    org_filter,
                    Some(chrome.t("flash-secret-not-configured")),
                    &chrome,
                )
                .await;
            }
        },
        None => (None, false),
    };

    let result = queries::integrations::create_integration(
        &state.writer_pool,
        active.session_org_id,
        &name,
        &kind,
        url.as_deref(),
        secret.as_deref(),
        config.as_deref(),
        encrypted,
        is_global,
    )
    .await;
    match result {
        Ok(_) => {
            render_list(
                &state,
                org_filter,
                Some(chrome.t("flash-integration-created")),
                &chrome,
            )
            .await
        }
        Err(ref e) if is_name_conflict(e) => {
            render_list(
                &state,
                org_filter,
                Some(chrome.t("flash-integration-name-exists")),
                &chrome,
            )
            .await
        }
        Err(e) => render_list(&state, org_filter, Some(chrome.err(e)), &chrome).await,
    }
}

/// What one project's routing resolves to for one integration.
#[derive(PartialEq)]
pub enum RoutingState {
    /// Global channel, no per-project row: delivering under org defaults.
    Default,
    Customised,
    Excluded,
    /// Tracker with no repository on this integration's forge and host.
    NoRepo,
    /// Non-global channel the project hasn't activated.
    NotRouted,
}

impl RoutingState {
    /// Fluent key for the state label.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Default => "integrations-state-default",
            Self::Customised => "integrations-state-customised",
            Self::Excluded => "integrations-state-excluded",
            Self::NoRepo => "integrations-state-no-repo",
            Self::NotRouted => "integrations-state-not-routed",
        }
    }

    pub fn is_delivering(&self) -> bool {
        matches!(self, Self::Default | Self::Customised)
    }
}

pub struct ProjectRoutingView {
    pub project_id: i64,
    pub label: String,
    pub archived: bool,
    pub state: RoutingState,
    pub excluded: bool,
}

#[derive(Template)]
#[template(path = "integration_detail.html")]
struct IntegrationDetailTemplate {
    integration: Integration,
    projects: Vec<ProjectRoutingView>,
    message: Option<String>,
    chrome: PageChrome,
    /// Only meaningful where inclusion is implicit: global channels and trackers.
    exclusion_applies: bool,
}

fn routing_state(
    row: &queries::integrations::ProjectRouting,
    integration: &Integration,
    has_repo: bool,
) -> RoutingState {
    if row.excluded {
        return RoutingState::Excluded;
    }
    if integration.kind.is_tracker() {
        return if has_repo {
            RoutingState::Default
        } else {
            RoutingState::NoRepo
        };
    }
    // `enabled` is ignored: nothing writes FALSE - `deactivate` deletes the row.
    match (row.customised, integration.is_global) {
        (true, _) => RoutingState::Customised,
        (false, true) => RoutingState::Default,
        (false, false) => RoutingState::NotRouted,
    }
}

pub async fn detail(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
    Path(id): Path<i64>,
) -> axum::response::Response {
    if let Err(r) = require_org_owner(&active) {
        return r;
    }
    render_detail(&state, &chrome, active.session_org_id, id, None).await
}

#[derive(Deserialize)]
pub struct ExcludeForm {
    pub project_id: i64,
}

/// Stop a global integration delivering to one project - there's no include list.
pub async fn exclude(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
    Path(id): Path<i64>,
    Form(form): Form<ExcludeForm>,
) -> axum::response::Response {
    exclusion_action(state, chrome, active, id, form.project_id, true).await
}

pub async fn un_exclude(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
    Path(id): Path<i64>,
    Form(form): Form<ExcludeForm>,
) -> axum::response::Response {
    exclusion_action(state, chrome, active, id, form.project_id, false).await
}

async fn exclusion_action(
    state: AppState,
    chrome: PageChrome,
    active: ActiveOrg,
    id: i64,
    project_id: i64,
    excluding: bool,
) -> axum::response::Response {
    if let Err(r) = require_org_owner(&active) {
        return r;
    }
    let org_id = active.session_org_id;

    // Both re-checked against the org, so a forged project_id can't cross orgs.
    let integration =
        match queries::integrations::get_integration(&state.pool, id, Some(org_id)).await {
            Ok(Some(i)) => i,
            Ok(None) => {
                return render_list(
                    &state,
                    Some(org_id),
                    Some(chrome.t("flash-integration-not-found")),
                    &chrome,
                )
                .await
            }
            Err(e) => return render_list(&state, Some(org_id), Some(chrome.err(e)), &chrome).await,
        };
    // Changing routing is configuration, so grace does not cover it.
    if !crate::commercial::providers::may_configure(&state.license, integration.kind) {
        return render_detail(
            &state,
            &chrome,
            org_id,
            id,
            Some(chrome.t("flash-integration-license-required")),
        )
        .await;
    }

    let in_org = queries::integrations::project_routing(&state.pool, org_id, id)
        .await
        .map(|rows| rows.iter().any(|r| r.project_id == project_id))
        .unwrap_or(false);
    if !in_org {
        return render_detail(
            &state,
            &chrome,
            org_id,
            id,
            Some(chrome.t("flash-project-not-found-or-denied")),
        )
        .await;
    }

    let result = if excluding {
        queries::integration_exclusions::exclude(&state.writer_pool, org_id, id, project_id)
            .await
            .map(|()| chrome.t("flash-project-excluded"))
    } else {
        queries::integration_exclusions::un_exclude(&state.writer_pool, org_id, id, project_id)
            .await
            .map(|_| chrome.t("flash-project-included"))
    };
    let msg = result.unwrap_or_else(|e| chrome.err(e));
    render_detail(&state, &chrome, org_id, id, Some(msg)).await
}

async fn render_detail(
    state: &AppState,
    chrome: &PageChrome,
    org_id: i64,
    id: i64,
    message: Option<String>,
) -> axum::response::Response {
    let integration =
        match queries::integrations::get_integration(&state.pool, id, Some(org_id)).await {
            Ok(Some(i)) => i,
            Ok(None) => {
                return render_list(
                    state,
                    Some(org_id),
                    Some(chrome.t("flash-integration-not-found")),
                    chrome,
                )
                .await
            }
            Err(e) => return render_list(state, Some(org_id), Some(chrome.err(e)), chrome).await,
        };

    let rows = queries::integrations::project_routing(&state.pool, org_id, id)
        .await
        .unwrap_or_default();

    // One org-wide repo query, matched in memory, not a resolve call per project.
    let repos = if integration.kind.is_tracker() {
        queries::projects::get_org_repos(&state.pool, org_id)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let projects = rows
        .iter()
        .map(|r| {
            let has_repo = repos.iter().any(|repo| {
                repo.project_id as i64 == r.project_id
                    && crate::trackers::tracker_repo_match(&integration, repo).is_some()
            });
            ProjectRoutingView {
                project_id: r.project_id,
                label: r
                    .name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| r.project_id.to_string()),
                archived: r.archived,
                state: routing_state(r, &integration, has_repo),
                excluded: r.excluded,
            }
        })
        .collect();

    let exclusion_applies = integration.is_global || integration.kind.is_tracker();
    render_template(&IntegrationDetailTemplate {
        integration,
        projects,
        message,
        chrome: chrome.clone(),
        exclusion_applies,
    })
}

#[derive(Deserialize)]
pub struct GlobalForm {
    #[serde(default)]
    pub is_global: Option<String>,
}

fn config_carries_recipient(integration: &Integration) -> bool {
    integration
        .config
        .as_deref()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
        .and_then(|v| v.get("to").and_then(|t| t.as_str()).map(str::to_string))
        .is_some_and(|to| !to.trim().is_empty())
}

/// Flips global routing in place - recreating would drop per-project overrides.
pub async fn set_global(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
    Path(id): Path<i64>,
    Form(form): Form<GlobalForm>,
) -> axum::response::Response {
    if let Err(r) = require_org_owner(&active) {
        return r;
    }
    let org_filter = active.role.as_ref().map(|_| active.session_org_id);

    let integration =
        match queries::integrations::get_integration(&state.pool, id, Some(active.session_org_id))
            .await
        {
            Ok(Some(i)) => i,
            Ok(None) => {
                return render_list(
                    &state,
                    org_filter,
                    Some(chrome.t("flash-integration-not-found")),
                    &chrome,
                )
                .await
            }
            Err(e) => return render_list(&state, org_filter, Some(chrome.err(e)), &chrome).await,
        };
    let kind = integration.kind;
    if !crate::commercial::providers::may_configure(&state.license, kind) {
        return render_list(
            &state,
            org_filter,
            Some(chrome.t("flash-integration-license-required")),
            &chrome,
        )
        .await;
    }
    if kind.is_tracker() {
        return render_list(
            &state,
            org_filter,
            Some(chrome.t("flash-integration-global-not-for-trackers")),
            &chrome,
        )
        .await;
    }

    let going_global = form.is_global.is_some();
    // Same rule `create` enforces: a global email integration needs a recipient.
    if going_global && kind == IntegrationKind::Email && !config_carries_recipient(&integration) {
        return render_list(
            &state,
            org_filter,
            Some(chrome.t("flash-global-email-needs-recipient")),
            &chrome,
        )
        .await;
    }

    let msg = match queries::integrations::set_global(
        &state.writer_pool,
        id,
        active.session_org_id,
        going_global,
    )
    .await
    {
        Ok(0) => chrome.t("flash-integration-not-found"),
        Ok(_) => chrome.t("flash-integration-saved"),
        Err(e) => chrome.err(e),
    };
    render_list(&state, org_filter, Some(msg), &chrome).await
}

pub async fn delete(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
    Path(id): Path<i64>,
) -> axum::response::Response {
    if let Err(r) = require_org_owner(&active) {
        return r;
    }
    let org_filter = active.role.as_ref().map(|_| active.session_org_id);
    let msg = match queries::integrations::delete_integration(
        &state.writer_pool,
        id,
        active.session_org_id,
    )
    .await
    {
        Ok(0) => format!(
            "{} {}",
            chrome.t("common-error-prefix"),
            chrome.tv1("flash-not-found-integration", "id", &id.to_string())
        ),
        Ok(_) => chrome.t("flash-integration-deleted"),
        Err(e) => chrome.err(e),
    };
    render_list(&state, org_filter, Some(msg), &chrome).await
}

/// Pick a recipient for the global email "Test" button: the acting user's email
/// when known, else the instance sender (send-to-self). `None` means neither is
/// available and the caller should surface an actionable flash.
fn resolve_email_test_recipient(
    email_cfg: &crate::config::EmailConfig,
    user_email: Option<&str>,
) -> Option<String> {
    user_email
        .map(str::to_string)
        .or_else(|| email_cfg.from_address.clone())
}

pub async fn test_integration(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
    Path(id): Path<i64>,
) -> axum::response::Response {
    if let Err(r) = require_org_owner(&active) {
        return r;
    }
    let org_filter = active.role.as_ref().map(|_| active.session_org_id);
    let integration =
        match queries::integrations::get_integration(&state.pool, id, org_filter).await {
            Ok(Some(i)) => i,
            Ok(None) => {
                return render_list(
                    &state,
                    org_filter,
                    Some(chrome.t("flash-integration-not-found")),
                    &chrome,
                )
                .await
            }
            Err(e) => return render_list(&state, org_filter, Some(chrome.err(e)), &chrome).await,
        };

    if let Some(r) = license_wall(&state, &chrome, org_filter, integration.kind).await {
        return r;
    }

    let secret = match (&integration.secret, integration.encrypted, &state.encryptor) {
        (Some(s), true, Some(enc)) => enc.decrypt(s),
        (Some(s), false, _) => Some(s.clone()),
        _ => None,
    };

    let event = crate::notify::NotificationEvent {
        trigger: crate::notify::NotifyTrigger::NewIssue,
        project_id: 0,
        // Empty: a test notification has no real issue, so no (dead) link is added.
        fingerprint: String::new(),
        title: Some("Test notification from Stackpit".to_string()),
        level: Some("info".to_string()),
        environment: Some("test".to_string()),
        environments: vec!["test".to_string()],
        event_id: "test-event-id".to_string(),
        digest: None,
    };

    let result = if integration.kind == "email" {
        // Endpoint isn't user-controlled -- no SSRF check or pinned client.
        match state.config.email.as_ref() {
            Some(email_cfg) => {
                // Recipients are per-project; the global test has none, so fall
                // back to the instance sender (send-to-self smoke test).
                match resolve_email_test_recipient(email_cfg, None) {
                    Some(to) => {
                        let project_cfg = serde_json::json!({ "to": to }).to_string();
                        crate::providers::email::send(
                            email_cfg,
                            &state.config.server.web_base(),
                            secret.as_deref(),
                            integration.config.as_deref(),
                            Some(project_cfg.as_str()),
                            &event,
                        )
                        .await
                    }
                    None => Err(anyhow::anyhow!(
                        "email integrations are tested with a recipient; configure [email] from_address or test from a project"
                    )),
                }
            }
            None => Err(anyhow::anyhow!(
                "email is not configured ([email] section absent)"
            )),
        }
    } else {
        let url = match integration.url.as_deref() {
            Some(u) if !u.is_empty() => u,
            _ => {
                return render_list(
                    &state,
                    org_filter,
                    Some(chrome.t("flash-integration-no-url")),
                    &chrome,
                )
                .await
            }
        };

        // Pin resolved DNS so reqwest can't re-resolve to a different (internal) IP.
        let resolved = match crate::util::ssrf::check_ssrf(url).await {
            Ok(r) => r,
            Err(msg) => return render_list(&state, org_filter, Some(msg), &chrome).await,
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
                return html_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error");
            }
        };

        crate::providers::dispatch(
            &state.license,
            &client,
            &integration.kind,
            url,
            secret.as_deref(),
            &event,
        )
        .await
    };

    match result {
        Ok(()) => {
            render_list(
                &state,
                org_filter,
                Some(chrome.t("flash-test-notification-sent")),
                &chrome,
            )
            .await
        }
        Err(e) => {
            render_list(
                &state,
                org_filter,
                Some(chrome.tv1("flash-test-failed", "error", &e.to_string())),
                &chrome,
            )
            .await
        }
    }
}

#[derive(Template)]
#[template(path = "integration_new_webhook.html")]
struct NewWebhookTemplate {
    chrome: PageChrome,
}

pub async fn new_webhook(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
) -> axum::response::Response {
    if let Err(r) = require_org_owner(&active) {
        return r;
    }
    let org_filter = active.role.as_ref().map(|_| active.session_org_id);
    if let Some(r) = license_wall(&state, &chrome, org_filter, IntegrationKind::Webhook).await {
        return r;
    }
    render_template(&NewWebhookTemplate { chrome })
}

#[derive(Template)]
#[template(path = "integration_new_slack.html")]
struct NewSlackTemplate {
    chrome: PageChrome,
}

pub async fn new_slack(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
) -> axum::response::Response {
    if let Err(r) = require_org_owner(&active) {
        return r;
    }
    let org_filter = active.role.as_ref().map(|_| active.session_org_id);
    if let Some(r) = license_wall(&state, &chrome, org_filter, IntegrationKind::Slack).await {
        return r;
    }
    render_template(&NewSlackTemplate { chrome })
}

#[derive(Template)]
#[template(path = "integration_new_email.html")]
struct NewEmailTemplate {
    chrome: PageChrome,
    lock: bool,
    default_provider: &'static str,
    from_placeholder: String,
    from_name_placeholder: String,
    /// Whether `[email] token` is set; if so the API-token form field is optional.
    has_default_token: bool,
    /// Whether `[email] from_address` is set; if so the From form field is optional.
    has_default_from: bool,
    /// Whether the instance provider is itself SMTP; gates the SMTP option.
    smtp_configured: bool,
}

pub async fn new_email(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
) -> axum::response::Response {
    use crate::providers::email as email_provider;
    if let Err(r) = require_org_owner(&active) {
        return r;
    }
    let org_filter = active.role.as_ref().map(|_| active.session_org_id);
    // No `[email]` section: mail is unconfigured, so there's nothing to add.
    let Some(email) = state.config.email.as_ref() else {
        return render_list(
            &state,
            org_filter,
            Some(chrome.t("flash-email-not-configured")),
            &chrome,
        )
        .await;
    };
    let provider_label = email_provider::provider_label(&email.provider);
    render_template(&NewEmailTemplate {
        chrome,
        lock: email.lock,
        default_provider: provider_label,
        from_placeholder: email
            .from_address
            .clone()
            .unwrap_or_else(|| "alerts@example.com".to_string()),
        from_name_placeholder: email
            .from_name
            .clone()
            .unwrap_or_else(|| "Stackpit Alerts".to_string()),
        // A global API token is only a fallback for its own provider.
        has_default_token: email_provider::global_api_token(&email.provider, provider_label)
            .is_some(),
        has_default_from: email.from_address.is_some(),
        smtp_configured: provider_label == "smtp",
    })
}

#[derive(Template)]
#[template(path = "integration_new_tracker.html")]
struct NewTrackerTemplate {
    chrome: PageChrome,
}

pub async fn new_tracker(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
) -> axum::response::Response {
    if let Err(r) = require_org_owner(&active) {
        return r;
    }
    let org_filter = active.role.as_ref().map(|_| active.session_org_id);
    // GitHub stands in for all three trackers; they share one feature.
    if let Some(r) = license_wall(&state, &chrome, org_filter, IntegrationKind::GitHub).await {
        return r;
    }
    render_template(&NewTrackerTemplate { chrome })
}

/// Sends an unlicensed operator back to the integrations list with an
/// explanation instead of a form they can't submit. `None` means "carry on".
///
/// `org_filter` must stay scoped: a Slack webhook URL is itself a credential.
async fn license_wall(
    state: &AppState,
    chrome: &PageChrome,
    org_filter: Option<i64>,
    kind: IntegrationKind,
) -> Option<axum::response::Response> {
    if crate::commercial::providers::may_configure(&state.license, kind) {
        return None;
    }
    Some(
        render_list(
            state,
            org_filter,
            Some(chrome.t("flash-integration-license-required")),
            chrome,
        )
        .await,
    )
}

/// Detect a duplicate integration name across SQLite and Postgres.
fn is_name_conflict(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(sqlx::Error::Database(db_err)) = cause.downcast_ref::<sqlx::Error>() {
            // SQLite: "UNIQUE constraint failed: integrations.name"
            if db_err.message().contains("integrations.name") {
                return true;
            }
            // Postgres unique_violation (only one unique constraint on this table)
            if db_err.code().as_deref() == Some("23505") {
                return true;
            }
        }
    }
    false
}

async fn render_list(
    state: &AppState,
    org_id: Option<i64>,
    message: Option<String>,
    chrome: &PageChrome,
) -> axum::response::Response {
    let integrations = queries::integrations::list_integrations(&state.pool, org_id)
        .await
        .unwrap_or_default();

    let tmpl = IntegrationsTemplate {
        integrations,
        message,
        chrome: chrome.clone(),
        // Slack stands in for every gated kind; they share one feature.
        integrations_licensed: crate::commercial::providers::may_configure(
            &state.license,
            IntegrationKind::Slack,
        ),
    };

    render_template(&tmpl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;
    use unic_langid::langid;

    // Every integrations template renders in en and de without a Fluent
    // placeholder leaking. Empty list exercises the empty-state branch.
    #[test]
    fn integrations_templates_render_without_missing_keys() {
        for locale in [langid!("en"), langid!("de")] {
            let chrome = PageChrome::new("csrf".into(), locale.clone(), "/web/projects/".into());
            let pages = [
                IntegrationsTemplate {
                    integrations: Vec::new(),
                    message: None,
                    chrome: chrome.clone(),
                    integrations_licensed: true,
                }
                .render()
                .expect("integrations list renders"),
                IntegrationsTemplate {
                    integrations: Vec::new(),
                    message: None,
                    chrome: chrome.clone(),
                    integrations_licensed: false,
                }
                .render()
                .expect("unlicensed integrations list renders"),
                NewWebhookTemplate {
                    chrome: chrome.clone(),
                }
                .render()
                .expect("webhook form renders"),
                NewSlackTemplate {
                    chrome: chrome.clone(),
                }
                .render()
                .expect("slack form renders"),
                NewEmailTemplate {
                    chrome: chrome.clone(),
                    lock: false,
                    default_provider: "lettermint",
                    from_placeholder: "alerts@example.com".into(),
                    from_name_placeholder: "Stackpit Alerts".into(),
                    has_default_token: false,
                    has_default_from: false,
                    smtp_configured: true,
                }
                .render()
                .expect("email form renders"),
                NewEmailTemplate {
                    chrome: chrome.clone(),
                    lock: true,
                    default_provider: "lettermint",
                    from_placeholder: "alerts@example.com".into(),
                    from_name_placeholder: "Stackpit Alerts".into(),
                    has_default_token: true,
                    has_default_from: true,
                    smtp_configured: true,
                }
                .render()
                .expect("locked email form renders"),
                NewTrackerTemplate {
                    chrome: chrome.clone(),
                }
                .render()
                .expect("tracker form renders"),
            ];
            for html in pages {
                assert!(
                    !html.contains(crate::i18n::MISSING_PREFIX),
                    "integrations ({locale}) leaked a missing localization key: {html}"
                );
            }

            // A leftover target field would silently do nothing.
            let tracker = NewTrackerTemplate {
                chrome: chrome.clone(),
            }
            .render()
            .expect("tracker form renders");
            for gone in ["name=\"owner\"", "name=\"repo\"", "name=\"project_id\""] {
                assert!(
                    !tracker.contains(gone),
                    "tracker form ({locale}) still collects {gone}"
                );
            }
        }
    }

    /// `project_integrations.integration_id` cascades, so recreating would drop them.
    #[tokio::test]
    async fn the_global_toggle_flips_the_flag_and_keeps_per_project_rows() {
        let pool = crate::db::open_test_pool().await;
        sqlx::query(crate::db::sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (5, 'acme', 'Acme')"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let id = queries::integrations::create_integration(
            &pool,
            5,
            "ops",
            "slack",
            Some("https://hooks.test/x"),
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap();
        queries::integrations::activate_project_integration(
            &pool,
            42,
            id,
            true,
            true,
            Some("error"),
            Some("prod"),
            None,
            true,
            true,
        )
        .await
        .unwrap();

        let (state, _chans) = crate::server::AppState::for_test(pool.clone());
        let active = ActiveOrg {
            session_org_id: 5,
            role: Some(crate::orgs::Role::Owner),
            org_name: None,
            memberships: vec![(5, crate::orgs::Role::Owner)],
        };
        let chrome = PageChrome::new("csrf".into(), langid!("en"), "/web/projects/".into());

        set_global(
            State(state.clone()),
            Chrome(chrome.clone()),
            active.clone(),
            Path(id),
            Form(GlobalForm {
                is_global: Some("1".into()),
            }),
        )
        .await;

        let after = queries::integrations::get_integration(&pool, id, Some(5))
            .await
            .unwrap()
            .expect("still there");
        assert!(after.is_global);

        let rows = queries::integrations::list_project_integrations(&pool, 42)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "the per-project row must survive the toggle");
        assert_eq!(rows[0].min_level.as_deref(), Some("error"));
        assert_eq!(rows[0].environment_filter.as_deref(), Some("prod"));

        set_global(
            State(state),
            Chrome(chrome),
            active,
            Path(id),
            Form(GlobalForm { is_global: None }),
        )
        .await;
        assert!(
            !queries::integrations::get_integration(&pool, id, Some(5))
                .await
                .unwrap()
                .unwrap()
                .is_global
        );
    }

    async fn org_with_integration(
        pool: &crate::db::DbPool,
        org_id: i64,
        slug: &str,
        name: &str,
        kind: &str,
        config: Option<&str>,
    ) -> i64 {
        sqlx::query(crate::db::sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (?1, ?2, ?2)
             ON CONFLICT (org_id) DO NOTHING"
        ))
        .bind(org_id)
        .bind(slug)
        .execute(pool)
        .await
        .unwrap();
        queries::integrations::create_integration(
            pool,
            org_id,
            name,
            kind,
            Some("https://hooks.test/x"),
            None,
            config,
            false,
            false,
        )
        .await
        .unwrap()
    }

    fn owner_session(org_id: i64) -> ActiveOrg {
        ActiveOrg {
            session_org_id: org_id,
            role: Some(crate::orgs::Role::Owner),
            org_name: None,
            memberships: vec![(org_id, crate::orgs::Role::Owner)],
        }
    }

    #[tokio::test]
    async fn the_add_forms_are_refused_to_a_non_owner() {
        let pool = crate::db::open_test_pool().await;
        let (state, _chans) = crate::server::AppState::for_test(pool.clone());
        let chrome = PageChrome::new("csrf".into(), langid!("en"), "/web/projects/".into());
        let member = ActiveOrg {
            session_org_id: 5,
            role: Some(crate::orgs::Role::Member),
            org_name: None,
            memberships: vec![(5, crate::orgs::Role::Member)],
        };

        for resp in [
            new_webhook(State(state.clone()), Chrome(chrome.clone()), member.clone()).await,
            new_slack(State(state.clone()), Chrome(chrome.clone()), member.clone()).await,
            new_tracker(State(state.clone()), Chrome(chrome.clone()), member.clone()).await,
            new_email(State(state.clone()), Chrome(chrome.clone()), member.clone()).await,
        ] {
            assert_eq!(resp.status(), axum::http::StatusCode::FORBIDDEN);
        }
    }

    /// An endpoint URL is a credential, so the wall must stay org-scoped.
    #[tokio::test]
    async fn the_license_wall_never_lists_another_orgs_integrations() {
        let pool = crate::db::open_test_pool().await;
        org_with_integration(&pool, 5, "acme", "ours-hook", "slack", None).await;
        org_with_integration(&pool, 6, "other", "theirs-secret-hook", "slack", None).await;

        let (mut state, _chans) = crate::server::AppState::for_test(pool.clone());
        state.license = crate::commercial::LicenseHandle::new(
            crate::commercial::license::LicenseStatus::Unlicensed,
            0,
        );
        let chrome = PageChrome::new("csrf".into(), langid!("en"), "/web/projects/".into());

        let resp = new_slack(State(state), Chrome(chrome), owner_session(5)).await;
        let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        let body = String::from_utf8_lossy(&body);
        assert!(
            body.contains("ours-hook"),
            "its own org's list still renders"
        );
        assert!(
            !body.contains("theirs-secret-hook"),
            "org 6's integration must never appear on org 5's license wall"
        );
    }

    #[tokio::test]
    async fn flipping_a_recipient_less_email_integration_to_global_is_refused() {
        let pool = crate::db::open_test_pool().await;
        let no_to = org_with_integration(
            &pool,
            5,
            "acme",
            "mail-no-to",
            "email",
            Some(r#"{"provider":"smtp"}"#),
        )
        .await;
        let with_to = org_with_integration(
            &pool,
            5,
            "acme",
            "mail-with-to",
            "email",
            Some(r#"{"provider":"smtp","to":"ops@example.com"}"#),
        )
        .await;

        let (state, _chans) = crate::server::AppState::for_test(pool.clone());
        let chrome = PageChrome::new("csrf".into(), langid!("en"), "/web/projects/".into());

        set_global(
            State(state.clone()),
            Chrome(chrome.clone()),
            owner_session(5),
            Path(no_to),
            Form(GlobalForm {
                is_global: Some("1".into()),
            }),
        )
        .await;
        assert!(
            !queries::integrations::get_integration(&pool, no_to, Some(5))
                .await
                .unwrap()
                .unwrap()
                .is_global,
            "an email integration with no recipient must not go global"
        );

        set_global(
            State(state),
            Chrome(chrome),
            owner_session(5),
            Path(with_to),
            Form(GlobalForm {
                is_global: Some("1".into()),
            }),
        )
        .await;
        assert!(
            queries::integrations::get_integration(&pool, with_to, Some(5))
                .await
                .unwrap()
                .unwrap()
                .is_global,
            "and one that carries a recipient still can"
        );
    }

    #[tokio::test]
    async fn the_global_toggle_is_refused_for_tracker_kinds() {
        let pool = crate::db::open_test_pool().await;
        sqlx::query(crate::db::sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (5, 'acme', 'Acme')"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let id = queries::integrations::create_integration(
            &pool,
            5,
            "gh",
            "github",
            Some("https://api.github.com"),
            Some("tok"),
            None,
            false,
            false,
        )
        .await
        .unwrap();

        let (state, _chans) = crate::server::AppState::for_test(pool.clone());
        set_global(
            State(state),
            Chrome(PageChrome::new(
                "csrf".into(),
                langid!("en"),
                "/web/projects/".into(),
            )),
            ActiveOrg {
                session_org_id: 5,
                role: Some(crate::orgs::Role::Owner),
                org_name: None,
                memberships: vec![(5, crate::orgs::Role::Owner)],
            },
            Path(id),
            Form(GlobalForm {
                is_global: Some("1".into()),
            }),
        )
        .await;

        assert!(
            !queries::integrations::get_integration(&pool, id, Some(5))
                .await
                .unwrap()
                .unwrap()
                .is_global,
            "a tracker must not be marked global"
        );
    }

    #[test]
    fn only_the_channel_forms_offer_the_global_checkbox() {
        let chrome = PageChrome::new("csrf".into(), langid!("en"), "/web/projects/".into());
        for html in [
            NewWebhookTemplate {
                chrome: chrome.clone(),
            }
            .render()
            .unwrap(),
            NewSlackTemplate {
                chrome: chrome.clone(),
            }
            .render()
            .unwrap(),
        ] {
            assert!(html.contains("name=\"is_global\""));
        }

        let tracker = NewTrackerTemplate {
            chrome: chrome.clone(),
        }
        .render()
        .unwrap();
        assert!(
            !tracker.contains("name=\"is_global\""),
            "trackers resolve structurally; the checkbox would be a lie"
        );
    }

    // Unlicensed: the gated kinds stay visible (so the upsell lands) but their
    // links are gone, while email is always reachable.
    #[test]
    fn unlicensed_list_hides_gated_links_but_keeps_email() {
        let chrome = PageChrome::new("csrf".into(), langid!("en"), "/web/projects/".into());
        let html = IntegrationsTemplate {
            integrations: Vec::new(),
            message: None,
            chrome,
            integrations_licensed: false,
        }
        .render()
        .expect("renders");

        for gated in ["new/webhook", "new/slack", "new/tracker"] {
            assert!(
                !html.contains(gated),
                "unlicensed page still links to {gated}: {html}"
            );
        }
        assert!(html.contains("/web/settings/integrations/new/email"));
        assert!(html.contains("/web/admin/license"));
    }

    #[test]
    fn licensed_list_links_every_kind() {
        let chrome = PageChrome::new("csrf".into(), langid!("en"), "/web/projects/".into());
        let html = IntegrationsTemplate {
            integrations: Vec::new(),
            message: None,
            chrome,
            integrations_licensed: true,
        }
        .render()
        .expect("renders");

        for kind in ["webhook", "slack", "email", "tracker"] {
            assert!(
                html.contains(&format!("/web/settings/integrations/new/{kind}")),
                "licensed page is missing the {kind} link: {html}"
            );
        }
    }

    async fn owner_of(pool: &crate::db::DbPool, org_id: i64, project_id: i64) -> ActiveOrg {
        sqlx::query(crate::db::sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (?1, ?2, ?2)
             ON CONFLICT (org_id) DO NOTHING"
        ))
        .bind(org_id)
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(crate::db::sql!(
            "INSERT INTO projects (project_id, name, org_id) VALUES (?1, ?2, ?3)
             ON CONFLICT (project_id) DO UPDATE SET org_id = excluded.org_id"
        ))
        .bind(project_id)
        .bind(format!("project-{project_id}"))
        .bind(org_id)
        .execute(pool)
        .await
        .unwrap();
        ActiveOrg {
            session_org_id: org_id,
            role: Some(crate::orgs::Role::Owner),
            org_name: None,
            memberships: vec![(org_id, crate::orgs::Role::Owner)],
        }
    }

    fn test_chrome() -> PageChrome {
        PageChrome::new("csrf".into(), langid!("en"), "/web/projects/".into())
    }

    fn routing_row(
        project_id: i64,
        customised: bool,
        enabled: bool,
        excluded: bool,
    ) -> queries::integrations::ProjectRouting {
        queries::integrations::ProjectRouting {
            project_id,
            name: None,
            archived: false,
            customised,
            enabled,
            excluded,
        }
    }

    fn integration_fixture(kind: &str, is_global: bool) -> Integration {
        Integration {
            id: 1,
            name: "fixture".into(),
            kind: kind.parse().unwrap(),
            url: Some("https://hooks.test/x".into()),
            secret: None,
            encrypted: false,
            config: None,
            created_at: 0,
            is_global,
        }
    }

    #[test]
    fn routing_state_covers_every_shape() {
        let global = integration_fixture("slack", true);
        let per_project = integration_fixture("slack", false);
        let tracker = integration_fixture("github", false);

        assert!(
            routing_state(&routing_row(1, true, true, true), &global, true)
                == RoutingState::Excluded
        );
        assert!(
            routing_state(&routing_row(1, false, true, true), &tracker, true)
                == RoutingState::Excluded
        );

        assert!(
            routing_state(&routing_row(1, false, true, false), &global, false)
                == RoutingState::Default
        );
        assert!(
            routing_state(&routing_row(1, true, true, false), &global, false)
                == RoutingState::Customised
        );
        // `enabled = FALSE` is unreachable, so it has no state of its own.
        assert!(
            routing_state(&routing_row(1, true, false, false), &global, false)
                == RoutingState::Customised
        );

        assert!(
            routing_state(&routing_row(1, false, true, false), &per_project, false)
                == RoutingState::NotRouted
        );
        assert!(
            routing_state(&routing_row(1, true, true, false), &per_project, false)
                == RoutingState::Customised
        );

        assert!(
            routing_state(&routing_row(1, true, true, false), &tracker, false)
                == RoutingState::NoRepo
        );
        assert!(
            routing_state(&routing_row(1, false, true, false), &tracker, true)
                == RoutingState::Default
        );
    }

    #[test]
    fn the_detail_page_renders_every_state_in_both_locales() {
        for locale in [langid!("en"), langid!("de")] {
            let chrome = PageChrome::new("csrf".into(), locale.clone(), "/web/projects/".into());
            let projects = [
                RoutingState::Default,
                RoutingState::Customised,
                RoutingState::Excluded,
                RoutingState::NoRepo,
                RoutingState::NotRouted,
            ]
            .into_iter()
            .enumerate()
            .map(|(i, state)| ProjectRoutingView {
                project_id: i as i64 + 1,
                label: format!("project-{i}"),
                archived: i == 0,
                excluded: state == RoutingState::Excluded,
                state,
            })
            .collect();

            let html = IntegrationDetailTemplate {
                integration: integration_fixture("slack", true),
                projects,
                message: None,
                chrome: chrome.clone(),
                exclusion_applies: true,
            }
            .render()
            .expect("detail page renders");
            assert!(
                !html.contains(crate::i18n::MISSING_PREFIX),
                "detail page ({locale}) leaked a missing localization key: {html}"
            );
            assert!(html.contains("/web/settings/integrations/1/exclude"));
            assert!(html.contains("/web/settings/integrations/1/include"));

            let bare = IntegrationDetailTemplate {
                integration: integration_fixture("webhook", false),
                projects: Vec::new(),
                message: Some("hi".into()),
                chrome,
                exclusion_applies: false,
            }
            .render()
            .expect("empty detail page renders");
            assert!(!bare.contains(crate::i18n::MISSING_PREFIX));
            assert!(
                !bare.contains("/exclude"),
                "exclusion has no meaning for a per-project integration"
            );
        }
    }

    #[tokio::test]
    async fn excluding_and_including_a_project_round_trips() {
        let pool = crate::db::open_test_pool().await;
        let active = owner_of(&pool, 5, 900).await;
        let id = queries::integrations::create_integration(
            &pool,
            5,
            "ops",
            "slack",
            Some("https://hooks.test/x"),
            None,
            None,
            false,
            true,
        )
        .await
        .unwrap();

        let (state, _chans) = crate::server::AppState::for_test(pool.clone());
        assert_eq!(
            queries::integrations::get_active_for_project(&pool, 900)
                .await
                .unwrap()
                .len(),
            1,
        );

        exclude(
            State(state.clone()),
            Chrome(test_chrome()),
            active.clone(),
            Path(id),
            Form(ExcludeForm { project_id: 900 }),
        )
        .await;
        assert!(queries::integration_exclusions::is_excluded(&pool, id, 900)
            .await
            .unwrap());
        assert!(queries::integrations::get_active_for_project(&pool, 900)
            .await
            .unwrap()
            .is_empty());

        un_exclude(
            State(state),
            Chrome(test_chrome()),
            active,
            Path(id),
            Form(ExcludeForm { project_id: 900 }),
        )
        .await;
        assert!(
            !queries::integration_exclusions::is_excluded(&pool, id, 900)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn excluding_a_project_from_another_org_is_refused() {
        let pool = crate::db::open_test_pool().await;
        let active = owner_of(&pool, 5, 901).await;
        owner_of(&pool, 6, 902).await;
        let id = queries::integrations::create_integration(
            &pool,
            5,
            "ops2",
            "slack",
            Some("https://hooks.test/y"),
            None,
            None,
            false,
            true,
        )
        .await
        .unwrap();

        let (state, _chans) = crate::server::AppState::for_test(pool.clone());
        exclude(
            State(state),
            Chrome(test_chrome()),
            active,
            Path(id),
            Form(ExcludeForm { project_id: 902 }),
        )
        .await;

        assert!(
            !queries::integration_exclusions::is_excluded(&pool, id, 902)
                .await
                .unwrap(),
            "a project outside the acting org must not be excludable"
        );
    }

    #[tokio::test]
    async fn project_routing_lists_every_project_in_the_org() {
        let pool = crate::db::open_test_pool().await;
        owner_of(&pool, 7, 910).await;
        owner_of(&pool, 7, 911).await;
        owner_of(&pool, 8, 912).await;
        let id = queries::integrations::create_integration(
            &pool,
            7,
            "ops3",
            "slack",
            Some("https://hooks.test/z"),
            None,
            None,
            false,
            true,
        )
        .await
        .unwrap();
        queries::integrations::activate_project_integration(
            &pool,
            910,
            id,
            true,
            true,
            Some("error"),
            None,
            None,
            true,
            true,
        )
        .await
        .unwrap();
        queries::integration_exclusions::exclude(&pool, 7, id, 911)
            .await
            .unwrap();

        let rows = queries::integrations::project_routing(&pool, 7, id)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2, "org 8's project must not appear");
        assert!(rows[0].customised && !rows[0].excluded);
        assert!(!rows[1].customised && rows[1].excluded);
    }

    #[tokio::test]
    async fn the_detail_page_separates_matching_repos_from_missing_ones() {
        let pool = crate::db::open_test_pool().await;
        let active = owner_of(&pool, 9, 920).await;
        owner_of(&pool, 9, 921).await;
        let id = queries::integrations::create_integration(
            &pool,
            9,
            "gh",
            "github",
            Some("https://api.github.com"),
            Some("tok"),
            None,
            false,
            false,
        )
        .await
        .unwrap();
        queries::projects::upsert_project_repo(
            &pool,
            920,
            "https://github.com/acme/web",
            "github",
            None,
            None,
            None,
        )
        .await
        .unwrap();
        queries::projects::upsert_project_repo(
            &pool,
            921,
            "https://github.example.com/acme/api",
            "github",
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let repos = queries::projects::get_org_repos(&pool, 9).await.unwrap();
        assert_eq!(repos.len(), 2);
        let integration = queries::integrations::get_integration(&pool, id, Some(9))
            .await
            .unwrap()
            .unwrap();
        let rows = queries::integrations::project_routing(&pool, 9, id)
            .await
            .unwrap();
        let states: Vec<_> = rows
            .iter()
            .map(|r| {
                let has_repo = repos.iter().any(|repo| {
                    repo.project_id as i64 == r.project_id
                        && crate::trackers::tracker_repo_match(&integration, repo).is_some()
                });
                routing_state(r, &integration, has_repo)
            })
            .collect();
        assert!(
            states[0] == RoutingState::Default,
            "github.com repo matches"
        );
        assert!(
            states[1] == RoutingState::NoRepo,
            "an Enterprise host must not match the github.com token"
        );

        let (state, _chans) = crate::server::AppState::for_test(pool.clone());
        let response = detail(State(state), Chrome(test_chrome()), active, Path(id)).await;
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    fn email_cfg(from: Option<&str>) -> crate::config::EmailConfig {
        crate::config::EmailConfig {
            enabled: true,
            from_address: from.map(String::from),
            from_name: None,
            lock: false,
            provider: polymail::ProviderConfig::Postmark { token: "t".into() },
        }
    }

    #[test]
    fn a_global_email_integration_must_carry_a_default_recipient() {
        assert_eq!(default_recipient(None, false), Ok(None));
        assert_eq!(default_recipient(Some("  "), false), Ok(None));
        assert_eq!(
            default_recipient(Some(" ops@example.com "), false),
            Ok(Some("ops@example.com".into())),
        );

        assert_eq!(
            default_recipient(None, true),
            Err("flash-global-email-needs-recipient"),
        );
        assert_eq!(
            default_recipient(Some("ops@example.com"), true),
            Ok(Some("ops@example.com".into())),
        );

        assert_eq!(
            default_recipient(Some("not-an-address"), true),
            Err("flash-invalid-to-address"),
        );
    }

    #[test]
    fn email_test_recipient_prefers_user_then_from_address() {
        let cfg = email_cfg(Some("alerts@stackpit.test"));
        assert_eq!(
            resolve_email_test_recipient(&cfg, Some("me@stackpit.test")).as_deref(),
            Some("me@stackpit.test"),
        );
        assert_eq!(
            resolve_email_test_recipient(&cfg, None).as_deref(),
            Some("alerts@stackpit.test"),
        );
        assert_eq!(resolve_email_test_recipient(&email_cfg(None), None), None);
    }
}
