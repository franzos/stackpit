use askama::Template;
use axum::extract::Query;
use serde::Deserialize;

use crate::extractors::ProjectPageCtx;
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::{period_to_timestamp, ListParams};
use crate::queries;
use crate::queries::types::{
    IssueSummary, PagedResult, Pagination, SpanAggregation, TransactionDistribution,
    TransactionInstance, TransactionSummary,
};
use crate::queries::ProjectNavCounts;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

/// Related issues shown on the transaction summary. A short list, because it is
/// a pointer into the issue stream rather than a replacement for it.
const RELATED_ISSUE_LIMIT: u32 = 10;

#[derive(Template)]
#[template(path = "transaction_list.html")]
struct TransactionListTemplate {
    project_id: u64,
    items: Vec<TransactionSummary>,
    sort: String,
    period: String,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

#[derive(Template)]
#[template(path = "transaction_detail.html")]
struct TransactionDetailTemplate {
    project_id: u64,
    name: String,
    op: Option<String>,
    period: String,
    distribution: Option<TransactionDistribution>,
    trend_data: String,
    spans: SpanAggregation,
    span_cap: usize,
    issues: Vec<IssueSummary>,
    result: PagedResult<TransactionInstance>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

#[derive(Deserialize)]
pub struct DetailParams {
    pub name: Option<String>,
    pub period: Option<String>,
    #[serde(flatten)]
    pub page: Pagination,
}

pub async fn list_handler(
    ctx: ProjectPageCtx,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    let sort = params.sort.clone().unwrap_or_else(|| "p95".to_string());
    let period = params.period.clone().unwrap_or_else(|| "7d".to_string());
    let since = period_to_timestamp(&period).unwrap_or(0);

    let items =
        queries::transactions::list_transactions(&ctx.pool, ctx.project_id, since, &sort).await?;

    Ok(render_template(&TransactionListTemplate {
        project_id: ctx.project_id,
        items,
        sort,
        period,
        nav: ctx.nav,
        chrome: ctx.chrome,
    }))
}

pub async fn detail_handler(
    ctx: ProjectPageCtx,
    Query(params): Query<DetailParams>,
) -> Result<axum::response::Response, HtmlError> {
    let name = params.name.unwrap_or_default();
    let period = params.period.clone().unwrap_or_else(|| "7d".to_string());
    let since = period_to_timestamp(&period).unwrap_or(0);
    let page = params.page.page();

    let result =
        queries::transactions::list_transaction_instances(&ctx.pool, ctx.project_id, &name, &page)
            .await?;
    let distribution =
        queries::transactions::transaction_distribution(&ctx.pool, ctx.project_id, &name, since)
            .await?;
    let trend = queries::transactions::transaction_percentile_trend(
        &ctx.pool,
        ctx.project_id,
        &name,
        since,
    )
    .await?;
    let trend_data = super::charts::trend_chart_json(
        &trend,
        &ctx.chrome.t("spans-col-p50"),
        &ctx.chrome.t("spans-col-p95"),
    );
    let spans =
        queries::transactions::transaction_span_breakdown(&ctx.pool, ctx.project_id, &name, since)
            .await?;
    let issues = queries::issues::list_issues_for_transaction(
        &ctx.pool,
        ctx.project_id,
        &name,
        since,
        RELATED_ISSUE_LIMIT,
    )
    .await?;
    let op = result.items.first().and_then(|i| i.op.clone());

    Ok(render_template(&TransactionDetailTemplate {
        project_id: ctx.project_id,
        name,
        op,
        period,
        distribution,
        trend_data,
        spans,
        span_cap: queries::spans::MAX_SPAN_GROUPS,
        issues,
        result,
        nav: ctx.nav,
        chrome: ctx.chrome,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::LanguageIdentifier;
    use crate::queries::types::{SpanAggRow, TransactionTrendPoint};
    use askama::Template;
    use unic_langid::langid;

    fn chrome_for(locale: LanguageIdentifier) -> PageChrome {
        PageChrome::new(
            String::new(),
            locale,
            "/web/projects/1/transactions/detail".into(),
        )
    }

    fn issue(fingerprint: &str) -> IssueSummary {
        IssueSummary {
            fingerprint: fingerprint.into(),
            project_id: 1,
            title: Some("TypeError: undefined is not a function".into()),
            level: Some("error".into()),
            first_seen: 100,
            last_seen: 200,
            event_count: 3,
            status: Default::default(),
            item_type: Default::default(),
            user_count: 1,
        }
    }

    // The summary borrows keys from spans.ftl and issues.ftl as well as its own
    // bundle, so a render across both locales is the guard on that reuse.
    #[test]
    fn transaction_detail_renders_without_missing_keys() {
        for lang in [langid!("en"), langid!("de")] {
            let tmpl = TransactionDetailTemplate {
                project_id: 1,
                name: "/checkout".into(),
                op: Some("http.server".into()),
                period: "7d".into(),
                distribution: None,
                trend_data: crate::html::charts::trend_chart_json(
                    &[TransactionTrendPoint {
                        bucket: 3600,
                        label: "Jul 20 01:00".into(),
                        count: 4,
                        p50_ms: 80,
                        p95_ms: 1200,
                        regressed: true,
                    }],
                    "p50",
                    "p95",
                ),
                spans: SpanAggregation {
                    groups: vec![SpanAggRow {
                        op: Some("db.query".into()),
                        description: Some("SELECT 1".into()),
                        count: 2,
                        p50_ms: 10,
                        p95_ms: 30,
                        avg_ms: 20,
                    }],
                    truncated: true,
                },
                span_cap: queries::spans::MAX_SPAN_GROUPS,
                issues: vec![issue("fp1")],
                result: PagedResult {
                    items: vec![TransactionInstance {
                        event_id: "e1".into(),
                        trace_id: Some("trace-a".into()),
                        duration_ms: Some(1500),
                        timestamp: 100,
                        op: Some("http.server".into()),
                        status: Some("ok".into()),
                    }],
                    total: 1,
                    offset: 0,
                    limit: 25,
                },
                nav: ProjectNavCounts::default(),
                chrome: chrome_for(lang.clone()),
            };
            let out = tmpl.render().expect("render");
            assert!(
                !out.contains(crate::i18n::MISSING_PREFIX),
                "missing localization key for {lang} in transaction_detail render"
            );
        }
    }

    // Both new panels hide rather than render empty, so a transaction with no
    // spans and no errors does not grow two headed-but-blank tables.
    #[test]
    fn empty_span_breakdown_and_issues_render_no_heading() {
        let tmpl = TransactionDetailTemplate {
            project_id: 1,
            name: "/checkout".into(),
            op: None,
            period: "7d".into(),
            distribution: None,
            trend_data: String::new(),
            spans: SpanAggregation::default(),
            span_cap: queries::spans::MAX_SPAN_GROUPS,
            issues: Vec::new(),
            result: PagedResult {
                items: Vec::new(),
                total: 0,
                offset: 0,
                limit: 25,
            },
            nav: ProjectNavCounts::default(),
            chrome: chrome_for(langid!("en")),
        };
        let out = tmpl.render().expect("render");
        assert!(!out.contains("Span breakdown"));
        assert!(!out.contains("Related issues"));
        assert!(!out.contains("Percentile trend"));
        assert!(!out.contains("data-chart"));
        assert!(!out.contains(crate::i18n::MISSING_PREFIX));
    }
}
