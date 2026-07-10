use askama::Template;
use axum::extract::{Form, Path, State};
use serde::Deserialize;

use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::{self, Chrome};
use crate::orgs::extractor::{require_owner, ActiveOrg};
use crate::queries;
use crate::queries::alerts::{AlertRule, DigestSchedule};
use crate::server::AppState;

#[allow(unused_imports)]
use crate::html::filters;

/// `(project_id, display_label)` rendered into the project selectors. We pass
/// it as a tuple so the template can read `.0` / `.1` directly without a
/// dedicated struct.
type ProjectOption = (u64, String);

#[derive(Template)]
#[template(path = "alerts.html")]
struct AlertsTemplate {
    alert_rules: Vec<AlertRule>,
    digest_schedules: Vec<DigestSchedule>,
    projects: Vec<ProjectOption>,
    message: Option<String>,
    chrome: PageChrome,
}

pub async fn handler(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active_org: ActiveOrg,
) -> axum::response::Response {
    render_page(&state, active_org.org_id, None, &chrome).await
}

// -- Alert rules -------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAlertRuleForm {
    pub project_id: Option<String>,
    pub fingerprint: Option<String>,
    pub threshold_count: i64,
    pub window_secs: i64,
    #[serde(default, deserialize_with = "utils::empty_string_as_none")]
    pub cooldown_secs: Option<i64>,
}

pub async fn create_alert_rule(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active_org: ActiveOrg,
    Form(form): Form<CreateAlertRuleForm>,
) -> axum::response::Response {
    if let Err(r) = require_owner(&active_org) {
        return r;
    }
    let project_id: Option<u64> = form
        .project_id
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| s.trim().parse().ok());
    let fingerprint = form
        .fingerprint
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());

    if let Some(pid) = project_id {
        if active_org.role.is_some()
            && crate::queries::orgs::assert_project_in_org(
                &state.pool,
                pid as i64,
                active_org.org_id,
            )
            .await
            .is_err()
        {
            return render_page(
                &state,
                active_org.org_id,
                Some(chrome.t("flash-project-not-found-or-denied")),
                &chrome,
            )
            .await;
        }
    }

    let s = state.clone();
    let org_id = active_org.org_id;
    let success = chrome.t("flash-alert-rule-created");
    let render_chrome = chrome.clone();
    utils::query_then_render(
        queries::alerts::create_alert_rule(
            &state.writer_pool,
            org_id,
            project_id,
            fingerprint.as_deref(),
            "threshold",
            Some(form.threshold_count),
            Some(form.window_secs),
            form.cooldown_secs.unwrap_or(3600),
        )
        .await,
        &chrome,
        &success,
        move |msg| async move { render_page(&s, org_id, msg, &render_chrome).await },
    )
    .await
}

pub async fn delete_alert_rule(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active_org: ActiveOrg,
    Path(id): Path<i64>,
) -> axum::response::Response {
    if let Err(r) = require_owner(&active_org) {
        return r;
    }
    let msg =
        match queries::alerts::delete_alert_rule(&state.writer_pool, id, active_org.org_id).await {
            Ok(0) => format!(
                "{} {}",
                chrome.t("common-error-prefix"),
                chrome.tv1("flash-not-found-alert-rule", "id", &id.to_string())
            ),
            Ok(_) => chrome.t("flash-alert-rule-deleted"),
            Err(e) => chrome.err(e),
        };
    render_page(&state, active_org.org_id, Some(msg), &chrome).await
}

// -- Digest schedules --------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateDigestForm {
    pub project_id: Option<String>,
    pub interval_secs: i64,
}

pub async fn create_digest_schedule(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active_org: ActiveOrg,
    Form(form): Form<CreateDigestForm>,
) -> axum::response::Response {
    if let Err(r) = require_owner(&active_org) {
        return r;
    }
    let project_id: Option<u64> = form
        .project_id
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| s.trim().parse().ok());

    if let Some(pid) = project_id {
        if active_org.role.is_some()
            && crate::queries::orgs::assert_project_in_org(
                &state.pool,
                pid as i64,
                active_org.org_id,
            )
            .await
            .is_err()
        {
            return render_page(
                &state,
                active_org.org_id,
                Some(chrome.t("flash-project-not-found-or-denied")),
                &chrome,
            )
            .await;
        }
    }

    let s = state.clone();
    let org_id = active_org.org_id;
    let success = chrome.t("flash-digest-schedule-created");
    let render_chrome = chrome.clone();
    utils::query_then_render(
        queries::alerts::create_digest_schedule(
            &state.writer_pool,
            org_id,
            project_id,
            form.interval_secs,
        )
        .await,
        &chrome,
        &success,
        move |msg| async move { render_page(&s, org_id, msg, &render_chrome).await },
    )
    .await
}

pub async fn delete_digest_schedule(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active_org: ActiveOrg,
    Path(id): Path<i64>,
) -> axum::response::Response {
    if let Err(r) = require_owner(&active_org) {
        return r;
    }
    let msg =
        match queries::alerts::delete_digest_schedule(&state.writer_pool, id, active_org.org_id)
            .await
        {
            Ok(0) => format!(
                "{} {}",
                chrome.t("common-error-prefix"),
                chrome.tv1("flash-not-found-digest-schedule", "id", &id.to_string())
            ),
            Ok(_) => chrome.t("flash-digest-schedule-deleted"),
            Err(e) => chrome.err(e),
        };
    render_page(&state, active_org.org_id, Some(msg), &chrome).await
}

/// Send a preview of a digest schedule now. Uses real activity in the window if
/// there is any; otherwise falls back to a clearly-labeled sample. Routes
/// through the normal dispatcher (via `notify_tx`), so only integrations with
/// digests enabled receive it.
pub async fn test_digest_schedule(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active_org: ActiveOrg,
    Path(id): Path<i64>,
) -> axum::response::Response {
    if let Err(r) = require_owner(&active_org) {
        return r;
    }
    let org_id = active_org.org_id;

    let schedule = match queries::alerts::get_digest_schedule(&state.pool, id).await {
        Ok(Some(s)) if s.org_id == org_id => s,
        Ok(_) => {
            return render_page(
                &state,
                org_id,
                Some(chrome.tv1("flash-not-found-digest-schedule", "id", &id.to_string())),
                &chrome,
            )
            .await
        }
        Err(e) => return render_page(&state, org_id, Some(chrome.err(e)), &chrome).await,
    };

    // Representative window: since the last send, or one interval back if it has
    // never sent. `now` is passed in so the preview matches a real cycle.
    let now = chrono::Utc::now().timestamp();
    let period_start = if schedule.last_sent > 0 {
        schedule.last_sent
    } else {
        now - schedule.interval_secs
    };

    let projects = queries::alerts::build_digest_data(
        &state.pool,
        period_start,
        now,
        org_id,
        schedule.project_id,
    )
    .await
    .unwrap_or_default();

    let msg = if !projects.is_empty() {
        let queued = queue_digest_previews(&state, period_start, now, projects, false).await;
        if queued > 0 {
            chrome.tv1("flash-test-digest-sent", "count", &queued.to_string())
        } else {
            chrome.t("flash-test-digest-no-target")
        }
    } else if let Some(pid) = schedule.project_id {
        // No activity, but a concrete project: send a labeled sample.
        let sample = vec![sample_digest_project(pid)];
        let queued = queue_digest_previews(&state, period_start, now, sample, true).await;
        if queued > 0 {
            chrome.t("flash-test-digest-sample")
        } else {
            chrome.t("flash-test-digest-no-target")
        }
    } else {
        // Global schedule with no activity: no concrete recipient to sample.
        chrome.t("flash-test-digest-no-target")
    };

    render_page(&state, org_id, Some(msg), &chrome).await
}

/// Queue one digest event per project that has a digest-enabled integration.
/// Returns how many projects were queued (the dispatcher fans each out to that
/// project's digest-enabled integrations).
async fn queue_digest_previews(
    state: &AppState,
    period_start: i64,
    period_end: i64,
    projects: Vec<crate::notify::DigestProject>,
    sample: bool,
) -> usize {
    let title = if sample {
        "Sample digest"
    } else {
        "Digest summary"
    };
    let mut queued = 0usize;
    for project in projects {
        let has_digest =
            queries::integrations::get_active_for_project(&state.pool, project.project_id)
                .await
                .unwrap_or_default()
                .iter()
                .any(|pi| pi.notify_digests);
        if !has_digest {
            continue;
        }
        let event = crate::notify::NotificationEvent {
            trigger: crate::notify::NotifyTrigger::Digest,
            project_id: project.project_id,
            fingerprint: String::new(),
            title: Some(title.to_string()),
            level: None,
            environment: None,
            environments: Vec::new(),
            event_id: String::new(),
            digest: Some(crate::notify::DigestPayload {
                period_start,
                period_end,
                projects: vec![project],
                sample,
            }),
        };
        if state.notify_tx.try_send(event).is_ok() {
            queued += 1;
        }
    }
    queued
}

/// Example digest data for the sample preview when a window has no real activity.
fn sample_digest_project(project_id: u64) -> crate::notify::DigestProject {
    use crate::notify::{DigestIssue, DigestProject};
    DigestProject {
        project_id,
        name: Some("Example project".to_string()),
        new_issues: vec![
            // Empty fingerprint: these are illustrative, so the email renders them
            // without a (dead) issue link.
            DigestIssue {
                fingerprint: String::new(),
                title: Some("TypeError: cannot read property 'id' of undefined".to_string()),
                level: Some("error".to_string()),
                event_count: 12,
                first_seen: 0,
            },
            DigestIssue {
                fingerprint: String::new(),
                title: Some("Timeout calling payment API".to_string()),
                level: Some("warning".to_string()),
                event_count: 4,
                first_seen: 0,
            },
        ],
        active_issues_count: 7,
        total_events: 128,
    }
}

// -- Render ------------------------------------------------------------------

async fn render_page(
    state: &AppState,
    org_id: i64,
    message: Option<String>,
    chrome: &PageChrome,
) -> axum::response::Response {
    let alert_rules = queries::alerts::list_alert_rules(&state.pool, None, Some(org_id))
        .await
        .unwrap_or_default();
    let digest_schedules = queries::alerts::list_digest_schedules(&state.pool, Some(org_id))
        .await
        .unwrap_or_default();

    // Project selector: name when set, else `Project {id}`. Sorted by label so
    // the dropdown stays scannable as project count grows.
    let mut projects: Vec<ProjectOption> =
        queries::projects::list_projects(&state.pool, org_id, None, None, None)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let label = p.name.unwrap_or_else(|| {
                    chrome.tv1("alerts-project-fallback", "id", &p.project_id.to_string())
                });
                (p.project_id, label)
            })
            .collect();
    projects.sort_by_key(|a| a.1.to_lowercase());

    let tmpl = AlertsTemplate {
        alert_rules,
        digest_schedules,
        projects,
        message,
        chrome: chrome.clone(),
    };

    render_template(&tmpl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;
    use unic_langid::langid;

    // Empty-collections render must not leak any Fluent placeholder in en or de.
    #[test]
    fn alerts_renders_without_missing_keys() {
        for locale in [langid!("en"), langid!("de")] {
            let tmpl = AlertsTemplate {
                alert_rules: Vec::new(),
                digest_schedules: Vec::new(),
                projects: Vec::new(),
                message: None,
                chrome: PageChrome::new("csrf".into(), locale.clone(), "/web/projects/".into()),
            };
            let html = tmpl.render().expect("alerts renders");
            assert!(
                !html.contains(crate::i18n::MISSING_PREFIX),
                "alerts ({locale}) leaked a missing localization key: {html}"
            );
        }
    }
}
