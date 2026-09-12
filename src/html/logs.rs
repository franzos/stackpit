use askama::Template;
use axum::extract::{Query, RawQuery};
use serde::Deserialize;

use crate::extractors::{BrowserDefaults, ProjectPageCtx};
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::{
    build_filter_qs, defaults_redirect, period_or_default, period_to_timestamp,
};
use crate::queries;
use crate::queries::types::{LogEntry, LogFilter, PagedResult, Pagination};
use crate::queries::ProjectNavCounts;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Deserialize)]
pub struct LogListParams {
    pub query: Option<String>,
    pub level: Option<String>,
    pub period: Option<String>,
    #[serde(flatten)]
    pub page: Pagination,
}

#[derive(Template)]
#[template(path = "log_list.html")]
struct LogListTemplate {
    project_id: u64,
    result: PagedResult<LogEntry>,
    query: String,
    level: String,
    period: String,
    filter_qs: String,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn list_handler(
    ctx: ProjectPageCtx,
    BrowserDefaults(defaults): BrowserDefaults,
    RawQuery(raw_qs): RawQuery,
    Query(params): Query<LogListParams>,
) -> Result<axum::response::Response, HtmlError> {
    if let Some(redirect) = defaults_redirect(
        &format!("/web/projects/{}/logs/", ctx.project_id),
        raw_qs.as_deref(),
        &defaults,
        &["period"],
    ) {
        return Ok(redirect);
    }
    let query_str = params.query.clone().unwrap_or_default();
    let level_str = params.level.clone().unwrap_or_default();
    let period_str = period_or_default(params.period.as_deref());

    let filter = LogFilter {
        level: params.level.filter(|s| !s.is_empty()),
        query: params.query.filter(|s| !s.is_empty()),
        trace_id: None,
        since_ts: period_to_timestamp(&period_str),
    };
    let page = params.page.page();

    let result = queries::logs::list_logs(&ctx.pool, ctx.project_id, &filter, &page).await?;

    // `period` rides in the pager links so paging past page 1 keeps the window.
    let (filter_qs, _) = build_filter_qs(
        &[
            ("query", &query_str),
            ("level", &level_str),
            ("period", &period_str),
        ],
        "",
    );

    Ok(render_template(&LogListTemplate {
        project_id: ctx.project_id,
        result,
        query: query_str,
        level: level_str,
        period: period_str,
        filter_qs,
        nav: ctx.nav,
        chrome: ctx.chrome,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::LanguageIdentifier;
    use unic_langid::langid;

    fn empty_template(locale: LanguageIdentifier) -> LogListTemplate {
        LogListTemplate {
            project_id: 1,
            result: PagedResult {
                items: Vec::<LogEntry>::new(),
                total: 0,
                offset: 0,
                limit: 25,
            },
            query: String::new(),
            level: String::new(),
            period: "7d".into(),
            filter_qs: String::new(),
            nav: ProjectNavCounts::default(),
            chrome: PageChrome::new(String::new(), locale, "/web/projects/".into()),
        }
    }

    // Empty-collection render must not leak an unresolved Fluent key in either locale.
    #[test]
    fn renders_without_missing_keys() {
        for lang in [langid!("en"), langid!("de")] {
            let out = empty_template(lang.clone()).render().expect("render");
            assert!(
                !out.contains(crate::i18n::MISSING_PREFIX),
                "missing localization key for {lang} in log_list render"
            );
        }
    }

    // Empty renders skip the count-bearing pagination, so exercise the cluster's plurals directly.
    #[test]
    fn counted_keys_resolve() {
        for lang in [langid!("en"), langid!("de")] {
            let chrome = PageChrome::new(String::new(), lang.clone(), "/web/projects/".into());
            for (id, n) in [
                ("logs-count", 1),
                ("logs-count", 5),
                ("metrics-count", 5),
                ("spans-count", 5),
                ("transactions-detail-count", 5),
                ("trace-detail-span-count", 5),
            ] {
                let s = chrome.tv_count(id, n);
                assert!(
                    !s.contains(crate::i18n::MISSING_PREFIX),
                    "missing {id} for {lang}"
                );
            }
        }
    }
}
