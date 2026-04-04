use askama::Template;
use axum::extract::{Query, RawQuery, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::extractors::{BrowserDefaults, ReadPool};
use crate::html::render_template;
use crate::html::utils::{build_filter_qs, defaults_redirect_url, ListParams};
use crate::queries;
use crate::queries::types::{EventFilter, Page, PagedResult};
use crate::server::AppState;

use super::html_error;

// askama needs these filters in scope for template derivation
use crate::html::filters;

#[derive(Template)]
#[template(path = "event_list.html")]
struct EventListTemplate {
    result: PagedResult<queries::EventSummary>,
    query: String,
    level: String,
    project_id: String,
    item_type: String,
    sort: String,
    filter_qs: String,
    base_qs: String,
}

pub async fn handler(
    BrowserDefaults(defaults): BrowserDefaults,
    RawQuery(raw_qs): RawQuery,
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    if let Some(url) = defaults_redirect_url(
        "/web/events/",
        raw_qs.as_deref(),
        &defaults,
        &["level", "item_type"],
    ) {
        return axum::response::Redirect::to(&url).into_response();
    }
    let query_str = params.query.clone().unwrap_or_default();
    let level_str = params.level.clone().unwrap_or_default();
    let project_id_str = params.project_id.map(|p| p.to_string()).unwrap_or_default();
    let item_type_str = params.item_type.clone().unwrap_or_default();
    let sort_str = params.sort.clone().unwrap_or_default();

    let filter = EventFilter {
        level: params.level.filter(|s| !s.is_empty()),
        project_id: params.project_id,
        query: params.query.filter(|s| !s.is_empty()),
        sort: params.sort.filter(|s| !s.is_empty()),
        item_type: params.item_type.filter(|s| !s.is_empty()),
    };
    let page = Page::new(params.offset, params.limit);

    let result = match queries::events::list_all_events(&pool, &filter, &page).await {
        Ok(r) => r,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let (base_qs, filter_qs) = build_filter_qs(
        &[
            ("query", &query_str),
            ("level", &level_str),
            ("project_id", &project_id_str),
            ("item_type", &item_type_str),
        ],
        &sort_str,
    );

    let tmpl = EventListTemplate {
        result,
        query: query_str,
        level: level_str,
        project_id: project_id_str,
        item_type: item_type_str,
        sort: sort_str,
        filter_qs,
        base_qs,
    };

    render_template(&tmpl)
}
