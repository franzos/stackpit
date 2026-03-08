use askama::Template;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::ListParams;
use crate::queries;
use crate::queries::types::{MetricBucket, MetricInfo, Page, PagedResult};
use crate::queries::ProjectNavCounts;

use super::html_error;

use crate::html::filters;

#[derive(Template)]
#[template(path = "metric_list.html")]
struct MetricListTemplate {
    project_id: u64,
    result: PagedResult<MetricInfo>,
    nav: ProjectNavCounts,
}

pub async fn list_handler(
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    let page = Page::new(params.offset, params.limit);
    let result = match queries::metrics::list_metrics(&pool, project_id, &page).await {
        Ok(r) => r,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = MetricListTemplate {
        project_id,
        result,
        nav,
    };
    render_template(&tmpl)
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
}

pub async fn detail_handler(
    ReadPool(pool): ReadPool,
    Path((project_id, raw_mri)): Path<(u64, String)>,
    Query(params): Query<DetailParams>,
) -> axum::response::Response {
    let mri = raw_mri
        .strip_prefix('/')
        .unwrap_or(&raw_mri)
        .trim_end_matches('/')
        .to_string();
    let buckets =
        match queries::metrics::get_metric_series(&pool, project_id, &mri, params.from, params.to)
            .await
        {
            Ok(b) => b,
            Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };

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
    };
    render_template(&tmpl)
}
