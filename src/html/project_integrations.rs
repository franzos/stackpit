use askama::Template;
use axum::extract::{Form, Path, State};
use serde::Deserialize;

use crate::extractors::ProjectPath;
use crate::html::chrome::PageChrome;
use crate::html::flash::{self, Flash};
use crate::html::render_template;
use crate::html::utils::{self, Chrome};
use crate::orgs::extractor::{require_project_owner, require_project_scope, ActiveOrg};
use crate::queries;
use crate::queries::types::{Integration, ProjectIntegration};
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

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

#[derive(Template)]
#[template(path = "project_integrations.html")]
struct ProjectIntegrationsTemplate {
    project_id: u64,
    active: Vec<ProjectIntegration>,
    /// Notification channels: they carry level, environment and digest options.
    available_channels: Vec<Integration>,
    /// Issue trackers: activating one only says "this project may file here";
    /// which repository it files into comes from the project's repositories,
    /// and none of the notify options mean anything.
    available_trackers: Vec<Integration>,
    message: Option<Flash>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn handler(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
) -> axum::response::Response {
    let scope = match require_project_scope(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    render_page(&state, project_id, None, &chrome, scope.org_id).await
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
    let scope = match require_project_owner(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    // Reject cross-org links: the integration must belong to the project's own org.
    match queries::integrations::get_integration(
        &state.pool,
        form.integration_id,
        Some(scope.org_id),
    )
    .await
    {
        Ok(Some(i)) => {
            if !crate::commercial::providers::may_configure(&state.license, i.kind) {
                return render_page(
                    &state,
                    project_id,
                    Some(chrome.flash_of(flash::INTEGRATION_LICENSE_REQUIRED)),
                    &chrome,
                    scope.org_id,
                )
                .await;
            }
        }
        Ok(None) => {
            return render_page(
                &state,
                project_id,
                Some(chrome.flash_of(flash::INTEGRATION_NOT_FOUND)),
                &chrome,
                scope.org_id,
            )
            .await;
        }
        Err(e) => {
            return render_page(
                &state,
                project_id,
                Some(chrome.flash_err(e)),
                &chrome,
                scope.org_id,
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
                Some(Flash::err(chrome.t(key))),
                &chrome,
                scope.org_id,
            )
            .await
        }
    };

    let s = state.clone();
    let org_id = scope.org_id;
    let success = Flash::ok(chrome.t("flash-integration-activated"));
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
        success,
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
    let scope = match require_project_owner(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };

    // Editing routing is configuration, so grace doesn't cover it. Resolved the
    // same way `test` does — `id` is the project_integrations row, not the
    // integration, so the kind has to come off the joined list.
    match queries::integrations::list_project_integrations(&state.pool, project_id).await {
        Ok(pis) => match pis.into_iter().find(|p| p.id == id) {
            Some(pi) => {
                if !crate::commercial::providers::may_configure(&state.license, pi.integration_kind)
                {
                    return render_page(
                        &state,
                        project_id,
                        Some(chrome.flash_of(flash::INTEGRATION_LICENSE_REQUIRED)),
                        &chrome,
                        scope.org_id,
                    )
                    .await;
                }
            }
            None => {
                return render_page(
                    &state,
                    project_id,
                    Some(chrome.flash_of(flash::INTEGRATION_NOT_FOUND)),
                    &chrome,
                    scope.org_id,
                )
                .await;
            }
        },
        Err(e) => {
            return render_page(
                &state,
                project_id,
                Some(chrome.flash_err(e)),
                &chrome,
                scope.org_id,
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
                Some(Flash::err(chrome.t(key))),
                &chrome,
                scope.org_id,
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
        Ok(0) => Flash::err(format!(
            "{} {}",
            chrome.t("common-error-prefix"),
            chrome.tv1("flash-not-found-project-integration", "id", &id.to_string())
        )),
        Ok(_) => Flash::ok(chrome.t("flash-integration-updated")),
        Err(e) => chrome.flash_err(e),
    };
    render_page(&state, project_id, Some(msg), &chrome, scope.org_id).await
}

pub async fn deactivate(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let scope = match require_project_owner(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let msg = match queries::integrations::deactivate_project_integration(
        &state.writer_pool,
        project_id as i64,
        id,
    )
    .await
    {
        Ok(0) => Flash::err(format!(
            "{} {}",
            chrome.t("common-error-prefix"),
            chrome.tv1("flash-not-found-project-integration", "id", &id.to_string())
        )),
        Ok(_) => Flash::ok(chrome.t("flash-integration-deactivated")),
        Err(e) => chrome.flash_err(e),
    };
    render_page(&state, project_id, Some(msg), &chrome, scope.org_id).await
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
    let scope = match require_project_owner(&active, &state.pool, project_id as i64).await {
        Ok(s) => s,
        Err(r) => return r,
    };

    let pis = match queries::integrations::list_project_integrations(&state.pool, project_id).await
    {
        Ok(v) => v,
        Err(e) => {
            return render_page(
                &state,
                project_id,
                Some(chrome.flash_err(e)),
                &chrome,
                scope.org_id,
            )
            .await
        }
    };
    let Some(pi) = pis.into_iter().find(|p| p.id == id) else {
        return render_page(
            &state,
            project_id,
            Some(chrome.flash_of(flash::INTEGRATION_NOT_FOUND)),
            &chrome,
            scope.org_id,
        )
        .await;
    };

    if !crate::commercial::providers::may_configure(&state.license, pi.integration_kind) {
        return render_page(
            &state,
            project_id,
            Some(chrome.flash_of(flash::INTEGRATION_LICENSE_REQUIRED)),
            &chrome,
            scope.org_id,
        )
        .await;
    }

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
                    Some(chrome.flash_of(flash::INTEGRATION_NO_URL)),
                    &chrome,
                    scope.org_id,
                )
                .await
            }
        };
        // Pin resolved DNS so reqwest can't re-resolve to an internal address.
        let resolved = match crate::util::ssrf::check_ssrf(url).await {
            Ok(r) => r,
            Err(msg) => {
                return render_page(
                    &state,
                    project_id,
                    Some(Flash::err(msg)),
                    &chrome,
                    scope.org_id,
                )
                .await
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
                    Some(Flash::err(chrome.tv1(
                        "flash-test-failed",
                        "error",
                        "internal error",
                    ))),
                    &chrome,
                    scope.org_id,
                )
                .await;
            }
        };
        crate::providers::dispatch(
            &state.license,
            &client,
            &pi.integration_kind,
            url,
            secret.as_deref(),
            &event,
        )
        .await
    };

    let msg = match result {
        Ok(()) => chrome.flash_of(flash::TEST_NOTIFICATION_SENT),
        Err(e) => Flash::err(chrome.tv1("flash-test-failed", "error", &e.to_string())),
    };
    render_page(&state, project_id, Some(msg), &chrome, scope.org_id).await
}

async fn render_page(
    state: &AppState,
    project_id: u64,
    message: Option<Flash>,
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
    // Split so each kind gets a form that only asks what it can use. The
    // post-activation row already branches on `is_tracker()`.
    let (available_trackers, available_channels): (Vec<_>, Vec<_>) =
        available.into_iter().partition(|i| i.kind.is_tracker());

    let nav = state.nav_counts(project_id).await;

    let tmpl = ProjectIntegrationsTemplate {
        project_id,
        active,
        available_channels,
        available_trackers,
        message,
        nav,
        chrome: chrome.clone(),
    };

    render_template(&tmpl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;
    use unic_langid::langid;

    fn integration(id: i64, name: &str, kind: crate::domain::IntegrationKind) -> Integration {
        Integration {
            id,
            name: name.into(),
            kind,
            url: Some("https://x.test/h".into()),
            secret: None,
            encrypted: false,
            config: None,
            created_at: 1_700_000_000,
            is_global: false,
        }
    }

    fn page(
        channels: Vec<Integration>,
        trackers: Vec<Integration>,
        locale: unic_langid::LanguageIdentifier,
    ) -> String {
        ProjectIntegrationsTemplate {
            project_id: 7,
            active: Vec::new(),
            available_channels: channels,
            available_trackers: trackers,
            message: None,
            nav: ProjectNavCounts::default(),
            chrome: PageChrome::new("csrf".into(), locale, "/web/projects/".into()),
        }
        .render()
        .expect("project integrations renders")
    }

    /// A tracker files on demand into the project's own repository, so none of
    /// the channel options apply. Offering them would be offering settings that
    /// do nothing.
    #[test]
    fn the_tracker_form_asks_only_for_the_integration() {
        use crate::domain::IntegrationKind as K;

        let html = page(
            Vec::new(),
            vec![integration(1, "gh", K::GitHub)],
            langid!("en"),
        );
        assert!(html.contains("Activate issue tracker"));
        for field in [
            r#"name="min_level""#,
            r#"name="environment_filter""#,
            r#"name="notify_threshold""#,
            r#"name="notify_digests""#,
            r#"name="to_address""#,
        ] {
            assert!(
                !html.contains(field),
                "the tracker form still offers {field}"
            );
        }
        assert!(html.contains(r#"name="integration_id""#));
    }

    /// The channel form keeps every option it had.
    #[test]
    fn the_channel_form_keeps_its_options() {
        use crate::domain::IntegrationKind as K;

        let html = page(
            vec![integration(2, "ops", K::Slack)],
            Vec::new(),
            langid!("en"),
        );
        for field in [
            r#"name="min_level""#,
            r#"name="environment_filter""#,
            r#"name="notify_threshold""#,
            r#"name="to_address""#,
        ] {
            assert!(html.contains(field), "the channel form lost {field}");
        }
        assert!(!html.contains("Activate issue tracker"));
    }

    /// Both lists empty is the only empty state; one list empty is not.
    #[test]
    fn the_empty_state_needs_both_lists_empty() {
        use crate::domain::IntegrationKind as K;

        // The active list is empty in both cases and has an empty state of its
        // own, so count the panels rather than looking for the class.
        for locale in [langid!("en"), langid!("de")] {
            let empty = page(Vec::new(), Vec::new(), locale.clone());
            assert!(!empty.contains(crate::i18n::MISSING_PREFIX));
            assert_eq!(
                empty.matches(r#"<p class="empty">"#).count(),
                2,
                "nothing active and nothing available: two empty states"
            );

            let one = page(
                Vec::new(),
                vec![integration(1, "gh", K::GitHub)],
                locale.clone(),
            );
            assert!(!one.contains(crate::i18n::MISSING_PREFIX), "{locale}");
            assert_eq!(
                one.matches(r#"<p class="empty">"#).count(),
                1,
                "a tracker is available, so only the active list is empty"
            );
        }
    }
}
