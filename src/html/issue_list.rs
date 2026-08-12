use askama::Template;
use axum::extract::{Query, RawQuery, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::db::DbPool;
use crate::extractors::{BrowserDefaults, ProjectPath, ReadPool};
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::{
    build_filter_qs, defaults_redirect_url, issue_filter_from_params, period_to_timestamp, Chrome,
    ListParams,
};
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::projects::NavCountsCache;
use crate::queries::types::PagedResult;
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::charts;
use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "issue_list.html")]
struct IssueListTemplate {
    project_id: u64,
    result: PagedResult<queries::IssueSummary>,
    query: String,
    level: String,
    status: String,
    sort: String,
    release: String,
    environment: String,
    tag: String,
    period: String,
    releases: Vec<String>,
    environments: Vec<String>,
    filter_qs: String,
    base_qs: String,
    nav: ProjectNavCounts,
    chart_data: String,
    // fingerprint -> inline trend sparkline SVG for the rows on this page.
    sparks: std::collections::HashMap<String, String>,
    chrome: PageChrome,
}

// axum extractors, not a real argument list
#[allow(clippy::too_many_arguments)]
pub async fn handler(
    active: ActiveOrg,
    State(state): State<AppState>,
    BrowserDefaults(defaults): BrowserDefaults,
    RawQuery(raw_qs): RawQuery,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    if let Some(url) = defaults_redirect_url(
        &format!("/web/projects/{project_id}/"),
        raw_qs.as_deref(),
        &defaults,
        &["status", "level", "period"],
    ) {
        return Ok(axum::response::Redirect::to(&url).into_response());
    }
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(StatusCode::NOT_FOUND, "Not found".into()))?;
    issue_or_transaction_handler(&pool, &state.nav_cache, project_id, params, "event", chrome).await
}

async fn issue_or_transaction_handler(
    pool: &DbPool,
    cache: &NavCountsCache,
    project_id: u64,
    params: ListParams,
    item_type: &str,
    chrome: PageChrome,
) -> Result<axum::response::Response, HtmlError> {
    let query_str = params.query.clone().unwrap_or_default();
    let level_str = params.level.clone().unwrap_or_default();
    let status_str = params.status.clone().unwrap_or_default();
    let sort_str = params.sort.clone().unwrap_or_default();
    let release_str = params.release.clone().unwrap_or_default();
    let environment_str = params.environment.clone().unwrap_or_default();
    let tag_str = params.tag.clone().unwrap_or_default();
    let period_str = params.period.clone().unwrap_or_else(|| "7d".to_string());

    let since = period_to_timestamp(&period_str);

    let filter = issue_filter_from_params(&params, item_type);
    let page = params.page.page();

    let result = queries::issues::list_issues(pool, project_id, &filter, &page, since).await?;

    let nav = queries::projects::nav_counts_cached(pool, cache, project_id).await;

    let releases = queries::releases::list_releases_for_project(pool, project_id)
        .await
        .unwrap_or_default();

    let environments = queries::events::list_environments_for_project(pool, project_id)
        .await
        .unwrap_or_default();

    let chart_data = match queries::events::project_event_histogram(
        pool,
        project_id,
        &filter,
        &period_str,
    )
    .await
    {
        Ok(buckets) => charts::chart_json(&buckets, "Events"),
        Err(_) => String::new(),
    };

    // Per-issue trend sparklines: 20 buckets across the active period, one query
    // for the whole page. Skipped for the "all time" window (no fixed start).
    let sparks = match period_to_timestamp(&period_str) {
        Some(start_ts) => {
            const BUCKETS: usize = 20;
            let now = chrono::Utc::now().timestamp();
            let bucket_secs = ((now - start_ts) / BUCKETS as i64).max(1);
            let fps: Vec<String> = result.items.iter().map(|i| i.fingerprint.clone()).collect();
            queries::issues::issue_sparklines(
                pool,
                project_id,
                &fps,
                start_ts,
                bucket_secs,
                BUCKETS,
            )
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(fp, counts)| (fp, charts::render_sparkline(&counts)))
            .collect()
        }
        None => std::collections::HashMap::new(),
    };

    let (base_qs, filter_qs) = build_filter_qs(
        &[
            ("query", &query_str),
            ("level", &level_str),
            ("status", &status_str),
            ("release", &release_str),
            ("environment", &environment_str),
            ("tag", &tag_str),
            ("period", &period_str),
        ],
        &sort_str,
    );

    Ok(render_template(&IssueListTemplate {
        project_id,
        result,
        query: query_str,
        level: level_str,
        status: status_str,
        sort: sort_str,
        release: release_str,
        environment: environment_str,
        tag: tag_str,
        period: period_str,
        releases,
        environments,
        filter_qs,
        base_qs,
        nav,
        chart_data,
        sparks,
        chrome,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::IssueStatus;
    use crate::queries::IssueSummary;
    use unic_langid::langid;

    fn issue(fp: &str) -> IssueSummary {
        IssueSummary {
            fingerprint: fp.into(),
            project_id: 1,
            title: Some("TypeError: boom".into()),
            level: Some("error".into()),
            first_seen: 1000,
            last_seen: 2000,
            event_count: 7,
            status: IssueStatus::Unresolved,
            item_type: Default::default(),
            user_count: 2,
        }
    }

    // Exercises the sparkline column end to end: the SVG is injected verbatim
    // for rows that have one and the row stays intact for those that don't.
    #[test]
    fn renders_rows_with_and_without_sparkline() {
        let mut sparks = std::collections::HashMap::new();
        sparks.insert(
            "fp-has".to_string(),
            "<svg class=\"spark\"><rect/></svg>".to_string(),
        );
        let tmpl = IssueListTemplate {
            project_id: 1,
            result: PagedResult {
                items: vec![issue("fp-has"), issue("fp-none")],
                total: 2,
                offset: 0,
                limit: 25,
            },
            query: String::new(),
            level: String::new(),
            status: String::new(),
            sort: String::new(),
            release: String::new(),
            environment: String::new(),
            tag: String::new(),
            period: "7d".into(),
            releases: Vec::new(),
            environments: vec!["production".into(), "staging".into()],
            filter_qs: String::new(),
            base_qs: String::new(),
            nav: ProjectNavCounts {
                label: "Proj".into(),
                ..Default::default()
            },
            chart_data: String::new(),
            sparks,
            chrome: PageChrome::new(String::new(), langid!("en"), "/web/projects/1/".into()),
        };
        let out = tmpl.render().expect("render");
        assert!(!out.contains(crate::i18n::MISSING_PREFIX));
        assert!(out.contains("<svg class=\"spark\"><rect/></svg>"));
        assert!(out.contains("fp-has") && out.contains("fp-none"));
    }
}
