use askama::Template;
use axum::extract::{Query, RawQuery, State};
use axum::response::IntoResponse;

use crate::extractors::{BrowserDefaults, ReadPool};
use crate::html::render_template;
use crate::html::utils::{
    build_filter_qs, defaults_redirect_url, event_filter_from_params, Csrf, ListParams,
};
use crate::queries;
use crate::queries::types::{Page, PagedResult};
use crate::server::AppState;

use super::HtmlError;

#[allow(unused_imports)]
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
    csrf_token: String,
}

pub async fn handler(
    BrowserDefaults(defaults): BrowserDefaults,
    RawQuery(raw_qs): RawQuery,
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    if let Some(url) = defaults_redirect_url(
        "/web/events/",
        raw_qs.as_deref(),
        &defaults,
        &["level", "item_type"],
    ) {
        return Ok(axum::response::Redirect::to(&url).into_response());
    }
    let query_str = params.query.clone().unwrap_or_default();
    let level_str = params.level.clone().unwrap_or_default();
    let project_id_str = params.project_id.map(|p| p.to_string()).unwrap_or_default();
    let item_type_str = params.item_type.clone().unwrap_or_default();
    let sort_str = params.sort.clone().unwrap_or_default();

    let filter = event_filter_from_params(&params);
    let page = Page::new(params.offset, params.limit);

    let result = queries::events::list_all_events(&pool, &filter, &page).await?;

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
        csrf_token: csrf,
    };

    Ok(render_template(&tmpl))
}
