use askama::Template;
use axum::extract::{Query, RawQuery, State};
use axum::response::IntoResponse;

use crate::extractors::{BrowserDefaults, ReadPool};
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::{
    build_filter_qs, defaults_redirect_url, event_filter_from_params, Chrome, ListParams,
};
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::PagedResult;
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
    chrome: PageChrome,
}

pub async fn handler(
    BrowserDefaults(defaults): BrowserDefaults,
    RawQuery(raw_qs): RawQuery,
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    Query(params): Query<ListParams>,
    active: ActiveOrg,
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
    let page = params.page.page();
    let org_id = if active.role.is_none() {
        None
    } else {
        Some(active.org_id)
    };

    let result = queries::events::list_all_events(&pool, &filter, &page, org_id).await?;

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
        chrome,
    };

    Ok(render_template(&tmpl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::LanguageIdentifier;
    use unic_langid::langid;

    fn empty_template(locale: LanguageIdentifier) -> EventListTemplate {
        EventListTemplate {
            result: PagedResult {
                items: Vec::<queries::EventSummary>::new(),
                total: 0,
                offset: 0,
                limit: 25,
            },
            query: String::new(),
            level: String::new(),
            project_id: String::new(),
            item_type: String::new(),
            sort: String::new(),
            filter_qs: String::new(),
            base_qs: String::new(),
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
                "missing localization key for {lang} in event_list render"
            );
        }
    }

    // The empty render skips the count-bearing pagination, so exercise those keys directly.
    #[test]
    fn counted_keys_resolve() {
        for lang in [langid!("en"), langid!("de")] {
            let chrome = PageChrome::new(String::new(), lang.clone(), "/web/projects/".into());
            for (id, n) in [
                ("events-count", 1),
                ("events-count", 5),
                ("events-bulk-delete-all", 3),
                ("events-bulk-delete-all-confirm", 3),
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
