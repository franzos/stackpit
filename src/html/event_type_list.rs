use askama::Template;
use axum::extract::Query;

use crate::extractors::ProjectPageCtx;
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::ListParams;
use crate::queries;
use crate::queries::types::{EventFilter, EventSummary, PagedResult};
use crate::queries::ProjectNavCounts;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "user_report_list.html")]
struct UserReportListTemplate {
    project_id: u64,
    result: PagedResult<EventSummary>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

#[derive(Template)]
#[template(path = "client_report_list.html")]
struct ClientReportListTemplate {
    project_id: u64,
    result: PagedResult<crate::queries::client_reports::ClientReportRow>,
    outcomes: Vec<crate::queries::client_reports::ClientReportOutcome>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn user_reports_handler(
    ctx: ProjectPageCtx,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    let filter = EventFilter {
        project_id: Some(ctx.project_id),
        item_type: Some("user_report".to_string()),
        ..Default::default()
    };
    let page = params.page.page();

    // project_id already pins scope; ProjectPageCtx enforces org membership
    let result = queries::events::list_all_events(&ctx.pool, &filter, &page, None).await?;

    let tmpl = UserReportListTemplate {
        project_id: ctx.project_id,
        result,
        nav: ctx.nav,
        chrome: ctx.chrome,
    };
    Ok(render_template(&tmpl))
}

pub async fn client_reports_handler(
    ctx: ProjectPageCtx,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    let page = params.page.page();

    // project_id already pins scope; ProjectPageCtx enforces org membership
    let result =
        queries::client_reports::list_client_reports(&ctx.pool, ctx.project_id, &page).await?;

    let since = chrono::Utc::now().timestamp() - 30 * 86400;
    let outcomes =
        queries::client_reports::summarize_client_reports(&ctx.pool, ctx.project_id, since).await?;

    let tmpl = ClientReportListTemplate {
        project_id: ctx.project_id,
        result,
        outcomes,
        nav: ctx.nav,
        chrome: ctx.chrome,
    };
    Ok(render_template(&tmpl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::LanguageIdentifier;
    use unic_langid::langid;

    fn empty_result() -> PagedResult<EventSummary> {
        PagedResult {
            items: Vec::new(),
            total: 0,
            offset: 0,
            limit: 25,
        }
    }

    fn chrome_for(locale: LanguageIdentifier) -> PageChrome {
        PageChrome::new(String::new(), locale, "/web/projects/1/".into())
    }

    // Empty-collection render must not leak an unresolved Fluent key in either locale.
    #[test]
    fn user_reports_render_without_missing_keys() {
        for lang in [langid!("en"), langid!("de")] {
            let tmpl = UserReportListTemplate {
                project_id: 1,
                result: empty_result(),
                nav: ProjectNavCounts::default(),
                chrome: chrome_for(lang.clone()),
            };
            let out = tmpl.render().expect("render");
            assert!(
                !out.contains(crate::i18n::MISSING_PREFIX),
                "missing localization key for {lang} in user_report_list render"
            );
        }
    }

    #[test]
    fn client_reports_render_without_missing_keys() {
        use crate::queries::client_reports::{ClientReportOutcome, ClientReportRow};
        // Populated so the reports table (and its new headers) actually renders.
        let result = PagedResult {
            items: vec![ClientReportRow {
                event_id: "abc123".into(),
                timestamp: 0,
                total_dropped: 15,
                outcomes: vec![ClientReportOutcome {
                    category: "session".into(),
                    reason: "network_error".into(),
                    quantity: 15,
                }],
            }],
            total: 1,
            offset: 0,
            limit: 25,
        };
        for lang in [langid!("en"), langid!("de")] {
            let tmpl = ClientReportListTemplate {
                project_id: 1,
                result: PagedResult {
                    items: result.items.clone(),
                    total: result.total,
                    offset: result.offset,
                    limit: result.limit,
                },
                outcomes: Vec::new(),
                nav: ProjectNavCounts::default(),
                chrome: chrome_for(lang.clone()),
            };
            let out = tmpl.render().expect("render");
            assert!(
                !out.contains(crate::i18n::MISSING_PREFIX),
                "missing localization key for {lang} in client_report_list render"
            );
        }
    }

    // The empty render skips the count-bearing pagination, so exercise those keys directly.
    #[test]
    fn counted_keys_resolve() {
        for lang in [langid!("en"), langid!("de")] {
            let chrome = chrome_for(lang.clone());
            for (id, n) in [
                ("client-reports-count", 1),
                ("client-reports-count", 5),
                ("client-reports-delete-all", 3),
                ("client-reports-delete-all-confirm", 3),
                ("user-reports-count", 1),
                ("user-reports-count", 5),
                ("user-reports-delete-all", 3),
                ("user-reports-delete-all-confirm", 3),
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
