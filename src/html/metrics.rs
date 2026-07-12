use askama::Template;
use axum::extract::{Path, Query, State};
use serde::Deserialize;

use crate::extractors::{ProjectPath, ReadPool};
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::{render_project_list, Chrome, ListParams};
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::{MetricBucket, MetricInfo, PagedResult};
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "metric_list.html")]
struct MetricListTemplate {
    project_id: u64,
    result: PagedResult<MetricInfo>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn list_handler(
    active: ActiveOrg,
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(axum::http::StatusCode::NOT_FOUND, "Not found".into()))?;
    let page = params.page.page();
    let result = queries::metrics::list_metrics(&pool, project_id, &page).await?;

    Ok(render_project_list(
        &pool,
        &state.nav_cache,
        project_id,
        chrome,
        result,
        |project_id, result, nav, chrome| MetricListTemplate {
            project_id,
            result,
            nav,
            chrome,
        },
    )
    .await)
}

#[derive(Deserialize)]
pub struct DetailParams {
    pub from: Option<i64>,
    pub to: Option<i64>,
}

#[derive(Template)]
#[template(path = "metric_detail.html")]
struct MetricDetailTemplate {
    project_id: u64,
    mri: String,
    metric_type: String,
    buckets: Vec<MetricBucket>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn detail_handler(
    active: ActiveOrg,
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    Path((project_id, raw_mri)): Path<(u64, String)>,
    Query(params): Query<DetailParams>,
) -> Result<axum::response::Response, HtmlError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(axum::http::StatusCode::NOT_FOUND, "Not found".into()))?;
    let mri = raw_mri
        .strip_prefix('/')
        .unwrap_or(&raw_mri)
        .trim_end_matches('/')
        .to_string();
    let buckets =
        queries::metrics::get_metric_series(&pool, project_id, &mri, params.from, params.to)
            .await?;

    let metric_type = queries::metrics::get_metric_type(&pool, project_id, &mri)
        .await
        .unwrap_or_default();

    let nav = state.nav_counts(project_id).await;

    let tmpl = MetricDetailTemplate {
        project_id,
        mri,
        metric_type,
        buckets,
        nav,
        chrome,
    };
    Ok(render_template(&tmpl))
}
