use askama::Template;
use axum::extract::{Form, Path, State};
use serde::Deserialize;

use crate::html::render_template;
use crate::html::utils::{self, Csrf};
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
    csrf_token: String,
}

pub async fn handler(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    active_org: ActiveOrg,
) -> axum::response::Response {
    render_page(&state, active_org.org_id, None, &csrf).await
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
    Csrf(csrf): Csrf,
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
                Some("Error: project not found or access denied".to_string()),
                &csrf,
            )
            .await;
        }
    }

    let s = state.clone();
    let org_id = active_org.org_id;
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
        "Alert rule created",
        move |msg| async move { render_page(&s, org_id, msg, &csrf).await },
    )
    .await
}

pub async fn delete_alert_rule(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    active_org: ActiveOrg,
    Path(id): Path<i64>,
) -> axum::response::Response {
    if let Err(r) = require_owner(&active_org) {
        return r;
    }
    let msg = match queries::alerts::delete_alert_rule(&state.writer_pool, id, active_org.org_id).await {
        Ok(0) => format!("Error: not found: alert rule: {id}"),
        Ok(_) => "Alert rule deleted".to_string(),
        Err(e) => format!("Error: {e}"),
    };
    render_page(&state, active_org.org_id, Some(msg), &csrf).await
}

// -- Digest schedules --------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateDigestForm {
    pub project_id: Option<String>,
    pub interval_secs: i64,
}

pub async fn create_digest_schedule(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
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
                Some("Error: project not found or access denied".to_string()),
                &csrf,
            )
            .await;
        }
    }

    let s = state.clone();
    let org_id = active_org.org_id;
    utils::query_then_render(
        queries::alerts::create_digest_schedule(
            &state.writer_pool,
            org_id,
            project_id,
            form.interval_secs,
        )
        .await,
        "Digest schedule created",
        move |msg| async move { render_page(&s, org_id, msg, &csrf).await },
    )
    .await
}

pub async fn delete_digest_schedule(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    active_org: ActiveOrg,
    Path(id): Path<i64>,
) -> axum::response::Response {
    if let Err(r) = require_owner(&active_org) {
        return r;
    }
    let msg = match queries::alerts::delete_digest_schedule(&state.writer_pool, id, active_org.org_id).await {
        Ok(0) => format!("Error: not found: digest schedule: {id}"),
        Ok(_) => "Digest schedule deleted".to_string(),
        Err(e) => format!("Error: {e}"),
    };
    render_page(&state, active_org.org_id, Some(msg), &csrf).await
}

// -- Render ------------------------------------------------------------------

async fn render_page(
    state: &AppState,
    org_id: i64,
    message: Option<String>,
    csrf: &str,
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
                let label = p
                    .name
                    .unwrap_or_else(|| format!("Project {}", p.project_id));
                (p.project_id, label)
            })
            .collect();
    projects.sort_by_key(|a| a.1.to_lowercase());

    let tmpl = AlertsTemplate {
        alert_rules,
        digest_schedules,
        projects,
        message,
        csrf_token: csrf.to_string(),
    };

    render_template(&tmpl)
}
