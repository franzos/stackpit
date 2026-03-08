use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};

use crate::html::html_error;
use crate::html::utils::period_to_timestamp;
use crate::queries::types::{EventFilter, IssueFilter};
use crate::queries::IssueStatus;
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

/// Bulk delete for the global events view.
pub async fn events_bulk(State(state): State<AppState>, body: Bytes) -> axum::response::Response {
    let form = match BulkForm::parse(&body) {
        Ok(f) => f,
        Err(e) => return html_error(StatusCode::BAD_REQUEST, &e),
    };

    if form.action != "delete" {
        return html_error(StatusCode::BAD_REQUEST, "Invalid action");
    }

    let (ids, filter, project_id) = if form.mode == "all_matching" {
        let f = EventFilter {
            level: opt(&form.level),
            project_id: None,
            query: opt(&form.query),
            sort: None,
            item_type: opt(&form.item_type),
        };
        (None, Some(f), None)
    } else {
        (Some(form.ids), None, None)
    };

    let reply_rx = match state.writer.bulk_delete_events(ids, filter, project_id) {
        Ok(rx) => rx,
        Err(e) => {
            return html_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Writer send failed: {e}"),
            )
        }
    };

    match reply_rx.await {
        Ok(Ok(_)) => Redirect::to("/web/events/").into_response(),
        Ok(Err(err)) => html_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()),
        Err(_) => html_error(StatusCode::INTERNAL_SERVER_ERROR, "Writer channel closed"),
    }
}

/// Bulk actions on issues -- delete, resolve, or ignore.
pub async fn issues_bulk(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    body: Bytes,
) -> axum::response::Response {
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

/// Bulk actions on transactions -- same as issues, different item type.
pub async fn transactions_bulk(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    body: Bytes,
) -> axum::response::Response {
    let form = match BulkForm::parse(&body) {
        Ok(f) => f,
        Err(e) => return html_error(StatusCode::BAD_REQUEST, &e),
    };
    handle_issue_bulk(
        state,
        project_id,
        "transaction",
        form,
        &format!("/web/projects/{project_id}/transactions/"),
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

    match form.action.as_str() {
        "delete" => {
            let (fingerprints, filter) = if form.mode == "all_matching" {
                let f = IssueFilter {
                    level: opt(&form.level),
                    status: opt(&form.status),
                    query: opt(&form.query),
                    sort: None,
                    item_type: Some(item_type.to_string()),
                    release: opt(&form.release),
                    tag: None,
                };
                (None, Some(f))
            } else {
                (Some(form.ids), None)
            };

            let reply_rx =
                match state
                    .writer
                    .bulk_delete_issues(fingerprints, filter, project_id, since)
                {
                    Ok(rx) => rx,
                    Err(e) => {
                        return html_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            &format!("Writer send failed: {e}"),
                        )
                    }
                };

            match reply_rx.await {
                Ok(Ok(_)) => Redirect::to(redirect_to).into_response(),
                Ok(Err(err)) => html_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()),
                Err(_) => html_error(StatusCode::INTERNAL_SERVER_ERROR, "Writer channel closed"),
            }
        }
        "resolve" | "ignore" => {
            let status = if form.action == "resolve" {
                IssueStatus::Resolved
            } else {
                IssueStatus::Ignored
            };

            let (fingerprints, filter) = if form.mode == "all_matching" {
                let f = IssueFilter {
                    level: opt(&form.level),
                    status: opt(&form.status),
                    query: opt(&form.query),
                    sort: None,
                    item_type: Some(item_type.to_string()),
                    release: opt(&form.release),
                    tag: None,
                };
                (None, Some(f))
            } else {
                (Some(form.ids), None)
            };

            let reply_rx = match state.writer.bulk_update_issue_status(
                fingerprints,
                filter,
                project_id,
                since,
                status,
            ) {
                Ok(rx) => rx,
                Err(e) => {
                    return html_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &format!("Writer send failed: {e}"),
                    )
                }
            };

            match reply_rx.await {
                Ok(Ok(_)) => Redirect::to(redirect_to).into_response(),
                Ok(Err(err)) => html_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()),
                Err(_) => html_error(StatusCode::INTERNAL_SERVER_ERROR, "Writer channel closed"),
            }
        }
        _ => html_error(
            StatusCode::BAD_REQUEST,
            &format!("Invalid action '{}'", form.action),
        ),
    }
}

/// Bulk delete for user reports.
pub async fn user_reports_bulk(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    body: Bytes,
) -> axum::response::Response {
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
    Path(project_id): Path<u64>,
    body: Bytes,
) -> axum::response::Response {
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

    let (ids, filter) = if form.mode == "all_matching" {
        let f = EventFilter {
            level: None,
            project_id: Some(project_id),
            query: None,
            sort: None,
            item_type: Some(item_type.to_string()),
        };
        (None, Some(f))
    } else {
        (Some(form.ids), None)
    };

    let reply_rx = match state
        .writer
        .bulk_delete_events(ids, filter, Some(project_id))
    {
        Ok(rx) => rx,
        Err(e) => {
            return html_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Writer send failed: {e}"),
            )
        }
    };

    match reply_rx.await {
        Ok(Ok(_)) => Redirect::to(redirect_to).into_response(),
        Ok(Err(err)) => html_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()),
        Err(_) => html_error(StatusCode::INTERNAL_SERVER_ERROR, "Writer channel closed"),
    }
}

/// Bulk delete for monitor check-ins.
pub async fn monitor_checkins_bulk(
    State(state): State<AppState>,
    Path((project_id, slug)): Path<(u64, String)>,
    body: Bytes,
) -> axum::response::Response {
    let form = match BulkForm::parse(&body) {
        Ok(f) => f,
        Err(e) => return html_error(StatusCode::BAD_REQUEST, &e),
    };

    if form.action != "delete" {
        return html_error(StatusCode::BAD_REQUEST, "Invalid action");
    }

    let (ids, filter) = if form.mode == "all_matching" {
        (
            None,
            Some(EventFilter {
                level: None,
                project_id: Some(project_id),
                query: None,
                sort: None,
                item_type: Some("check_in".to_string()),
            }),
        )
    } else {
        (Some(form.ids), None)
    };

    let reply_rx = match state
        .writer
        .bulk_delete_events(ids, filter, Some(project_id))
    {
        Ok(rx) => rx,
        Err(e) => {
            return html_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Writer send failed: {e}"),
            )
        }
    };

    let redirect = format!("/web/projects/{project_id}/monitors/{slug}/");
    match reply_rx.await {
        Ok(Ok(_)) => Redirect::to(&redirect).into_response(),
        Ok(Err(err)) => html_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()),
        Err(_) => html_error(StatusCode::INTERNAL_SERVER_ERROR, "Writer channel closed"),
    }
}
