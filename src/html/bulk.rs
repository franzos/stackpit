use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};

use crate::domain::IssueStatus;
use crate::html::html_error;
use crate::html::utils::period_to_timestamp;
use crate::orgs::extractor::{require_owner, require_project_scope, ActiveOrg};
use crate::queries;
use crate::queries::types::{EventFilter, IssueFilter};
use crate::server::AppState;

pub struct BulkForm {
    action: String,
    mode: String,
    ids: Vec<String>,
    query: Option<String>,
    level: Option<String>,
    status: Option<String>,
    item_type: Option<String>,
    release: Option<String>,
    period: Option<String>,
}

impl BulkForm {
    fn parse(body: &[u8]) -> Result<Self, String> {
        let mut action = None;
        let mut mode = None;
        let mut ids = Vec::new();
        let mut query = None;
        let mut level = None;
        let mut status = None;
        let mut item_type = None;
        let mut release = None;
        let mut period = None;

        for (key, val) in form_urlencoded::parse(body) {
            match key.as_ref() {
                "action" => action = Some(val.into_owned()),
                "mode" => mode = Some(val.into_owned()),
                "ids" => {
                    let v = val.trim();
                    if !v.is_empty() {
                        ids.push(v.to_string());
                    }
                }
                "query" => query = Some(val.into_owned()),
                "level" => level = Some(val.into_owned()),
                "status" => status = Some(val.into_owned()),
                "item_type" => item_type = Some(val.into_owned()),
                "release" => release = Some(val.into_owned()),
                "period" => period = Some(val.into_owned()),
                _ => {}
            }
        }

        Ok(BulkForm {
            action: action.ok_or("missing field: action")?,
            mode: mode.ok_or("missing field: mode")?,
            ids,
            query,
            level,
            status,
            item_type,
            release,
            period,
        })
    }
}

fn opt(s: &Option<String>) -> Option<String> {
    s.as_ref().filter(|s| !s.is_empty()).cloned()
}

/// Splits a bulk form into either explicit ids or an all-matching filter.
/// `build_filter` is only evaluated in the all-matching arm.
fn resolve_targets<T>(
    form: BulkForm,
    build_filter: impl FnOnce(&BulkForm) -> T,
) -> (Option<Vec<String>>, Option<T>) {
    if form.mode == "all_matching" {
        let filter = build_filter(&form);
        (None, Some(filter))
    } else {
        (Some(form.ids), None)
    }
}

/// Shared Ok->redirect / Err->500 tail for the bulk query calls.
fn bulk_result_redirect(
    result: anyhow::Result<impl Sized>,
    redirect_to: &str,
) -> axum::response::Response {
    match result {
        Ok(_) => Redirect::to(redirect_to).into_response(),
        Err(err) => html_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()),
    }
}

/// Bulk delete for the global events view.
pub async fn events_bulk(
    State(state): State<AppState>,
    active: ActiveOrg,
    body: Bytes,
) -> axum::response::Response {
    if let Err(r) = require_owner(&active) {
        return r;
    }

    let form = match BulkForm::parse(&body) {
        Ok(f) => f,
        Err(e) => return html_error(StatusCode::BAD_REQUEST, &e),
    };

    if form.action != "delete" {
        return html_error(StatusCode::BAD_REQUEST, "Invalid action");
    }

    // Scoped users (role Some) are constrained to their org; superusers (role None) are global.
    let org_id = active.role.as_ref().map(|_| active.org_id);

    let (ids, filter) = resolve_targets(form, |form| EventFilter {
        level: opt(&form.level),
        project_id: None,
        query: opt(&form.query),
        sort: None,
        item_type: opt(&form.item_type),
    });

    let result = queries::bulk::bulk_delete_events(
        &state.writer_pool,
        ids.as_deref(),
        filter.as_ref(),
        None,
        org_id,
    )
    .await;
    bulk_result_redirect(result, "/web/events/")
}

/// Bulk actions on issues -- delete, resolve, or ignore.
pub async fn issues_bulk(
    State(state): State<AppState>,
    active: ActiveOrg,
    Path(project_id): Path<u64>,
    body: Bytes,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }
    let form = match BulkForm::parse(&body) {
        Ok(f) => f,
        Err(e) => return html_error(StatusCode::BAD_REQUEST, &e),
    };
    handle_issue_bulk(
        state,
        project_id,
        "event",
        form,
        &format!("/web/projects/{project_id}/"),
    )
    .await
}

async fn handle_issue_bulk(
    state: AppState,
    project_id: u64,
    item_type: &str,
    form: BulkForm,
    redirect_to: &str,
) -> axum::response::Response {
    let since = form
        .period
        .as_deref()
        .map(period_to_timestamp)
        .unwrap_or(None);

    let status = match form.action.as_str() {
        "delete" => None,
        "resolve" => Some(IssueStatus::Resolved),
        "ignore" => Some(IssueStatus::Ignored),
        _ => {
            return html_error(
                StatusCode::BAD_REQUEST,
                &format!("Invalid action '{}'", form.action),
            )
        }
    };

    let (fingerprints, filter) = resolve_targets(form, |form| IssueFilter {
        level: opt(&form.level),
        status: opt(&form.status),
        query: opt(&form.query),
        sort: None,
        item_type: Some(item_type.to_string()),
        release: opt(&form.release),
        tag: None,
    });

    let result = if let Some(status) = status {
        queries::bulk::bulk_update_issue_status(
            &state.writer_pool,
            fingerprints.as_deref(),
            filter.as_ref(),
            project_id,
            since,
            status,
        )
        .await
        .map(|_| ())
    } else {
        queries::bulk::bulk_delete_issues(
            &state.writer_pool,
            fingerprints.as_deref(),
            filter.as_ref(),
            project_id,
            since,
        )
        .await
        .map(|_| ())
    };
    bulk_result_redirect(result, redirect_to)
}

/// Bulk delete for user reports.
pub async fn user_reports_bulk(
    State(state): State<AppState>,
    active: ActiveOrg,
    Path(project_id): Path<u64>,
    body: Bytes,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }
    let form = match BulkForm::parse(&body) {
        Ok(f) => f,
        Err(e) => return html_error(StatusCode::BAD_REQUEST, &e),
    };
    handle_event_type_bulk(
        state,
        project_id,
        "user_report",
        form,
        &format!("/web/projects/{project_id}/user-reports/"),
    )
    .await
}

/// Bulk delete for client reports.
pub async fn client_reports_bulk(
    State(state): State<AppState>,
    active: ActiveOrg,
    Path(project_id): Path<u64>,
    body: Bytes,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }
    let form = match BulkForm::parse(&body) {
        Ok(f) => f,
        Err(e) => return html_error(StatusCode::BAD_REQUEST, &e),
    };
    handle_event_type_bulk(
        state,
        project_id,
        "client_report",
        form,
        &format!("/web/projects/{project_id}/client-reports/"),
    )
    .await
}

async fn handle_event_type_bulk(
    state: AppState,
    project_id: u64,
    item_type: &str,
    form: BulkForm,
    redirect_to: &str,
) -> axum::response::Response {
    if form.action != "delete" {
        return html_error(StatusCode::BAD_REQUEST, "Invalid action");
    }

    let (ids, filter) = resolve_targets(form, |_| EventFilter {
        level: None,
        project_id: Some(project_id),
        query: None,
        sort: None,
        item_type: Some(item_type.to_string()),
    });

    let result = queries::bulk::bulk_delete_events(
        &state.writer_pool,
        ids.as_deref(),
        filter.as_ref(),
        Some(project_id),
        None,
    )
    .await;
    bulk_result_redirect(result, redirect_to)
}

/// Bulk delete for monitor check-ins.
pub async fn monitor_checkins_bulk(
    State(state): State<AppState>,
    active: ActiveOrg,
    Path((project_id, slug)): Path<(u64, String)>,
    body: Bytes,
) -> axum::response::Response {
    if let Err(r) = require_project_scope(&active, &state.pool, project_id as i64).await {
        return r;
    }
    if let Err(r) = require_owner(&active) {
        return r;
    }

    let form = match BulkForm::parse(&body) {
        Ok(f) => f,
        Err(e) => return html_error(StatusCode::BAD_REQUEST, &e),
    };

    if form.action != "delete" {
        return html_error(StatusCode::BAD_REQUEST, "Invalid action");
    }

    let (ids, filter) = resolve_targets(form, |_| EventFilter {
        level: None,
        project_id: Some(project_id),
        query: None,
        sort: None,
        item_type: Some("check_in".to_string()),
    });

    let redirect = format!("/web/projects/{project_id}/monitors/{slug}/");
    let result = queries::bulk::bulk_delete_events(
        &state.writer_pool,
        ids.as_deref(),
        filter.as_ref(),
        Some(project_id),
        None,
    )
    .await;
    bulk_result_redirect(result, &redirect)
}
