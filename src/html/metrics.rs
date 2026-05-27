use askama::Template;
use axum::extract::{Path, Query};
use serde::Deserialize;

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::{render_project_list, Csrf, ListParams};
use crate::queries;
use crate::queries::types::{MetricBucket, MetricInfo, Page, PagedResult};
use crate::queries::ProjectNavCounts;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "metric_list.html")]
struct MetricListTemplate {
    project_id: u64,
    result: PagedResult<MetricInfo>,
    nav: ProjectNavCounts,
    csrf_token: String,
}

pub async fn list_handler(
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    let page = Page::new(params.offset, params.limit);
    let result = queries::metrics::list_metrics(&pool, project_id, &page).await?;

    Ok(render_project_list(
        &pool,
        project_id,
        csrf,
        result,
        |project_id, result, nav, csrf_token| MetricListTemplate {
            project_id,
            result,
            nav,
            csrf_token,
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
    csrf_token: String,
}

pub async fn detail_handler(
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path((project_id, raw_mri)): Path<(u64, String)>,
    Query(params): Query<DetailParams>,
) -> Result<axum::response::Response, HtmlError> {
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

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = MetricDetailTemplate {
        project_id,
        mri,
        metric_type,
        buckets,
        nav,
        csrf_token: csrf,
    };
    Ok(render_template(&tmpl))
}
