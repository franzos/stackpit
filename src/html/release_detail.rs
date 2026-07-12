use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;

use crate::extractors::ReadPool;
use crate::html::chrome::PageChrome;
use crate::html::release_health::ReleaseHealthRow;
use crate::html::render_template;
use crate::html::utils::{Chrome, ListParams};
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::{IssueFilter, Page, PagedResult};
use crate::queries::{IssueSummary, ProjectNavCounts};
use crate::server::AppState;

use super::HtmlError;

// askama needs the filters in scope for template derivation
#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "release_detail.html")]
struct ReleaseDetailTemplate {
    project_id: u64,
    version: String,
    health: Option<ReleaseHealthRow>,
    issues: PagedResult<IssueSummary>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn handler(
    active: ActiveOrg,
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    Path((project_id, version)): Path<(u64, String)>,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(StatusCode::NOT_FOUND, "Not found".into()))?;

    // Session health for just this version, reusing the health page's defensive
    // crash-free recompute. The per-project set is small, so filter in Rust.
    let health = queries::releases::get_release_health(&pool, project_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|r| r.release == version)
        .map(ReleaseHealthRow::from);

    // Distinct error groups (issues) seen with this release.
    let filter = IssueFilter {
        item_type: Some("event".to_string()),
        release: Some(version.clone()),
        ..Default::default()
    };
    let page = Page::new(params.page.offset, params.page.limit.or(Some(50)));
    let issues = queries::issues::list_issues(&pool, project_id, &filter, &page, None).await?;

    let nav = state.nav_counts(project_id).await;

    let tmpl = ReleaseDetailTemplate {
        project_id,
        version,
        health,
        issues,
        nav,
        chrome,
    };
    Ok(render_template(&tmpl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::LanguageIdentifier;
    use unic_langid::langid;

    fn sample_health() -> ReleaseHealthRow {
        ReleaseHealthRow {
            release: "app@4646cc9bb4e8629a3f34c75b152f2abe1da1082a".into(),
            total_sessions: 73,
            ok_count: 68,
            crashed_count: 0,
            errored_count: 5,
            crash_free_sessions: Some(100.0),
            error_free_sessions: Some(93.15),
            crash_free_users: Some(100.0),
        }
    }

    fn empty_issues() -> PagedResult<IssueSummary> {
        PagedResult {
            items: Vec::new(),
            total: 0,
            offset: 0,
            limit: 50,
        }
    }

    fn chrome_for(locale: LanguageIdentifier) -> PageChrome {
        PageChrome::new(String::new(), locale, "/web/projects/1/".into())
    }

    // Renders in both tested locales with and without health, and must not leak
    // an unresolved Fluent key (new keys fall back to en).
    #[test]
    fn release_detail_renders_without_missing_keys() {
        for lang in [langid!("en"), langid!("de")] {
            for health in [Some(sample_health()), None] {
                let tmpl = ReleaseDetailTemplate {
                    project_id: 1,
                    version: "app@4646cc9bb4e8629a3f34c75b152f2abe1da1082a".into(),
                    health,
                    issues: empty_issues(),
                    nav: ProjectNavCounts::default(),
                    chrome: chrome_for(lang.clone()),
                };
                let out = tmpl.render().expect("render");
                assert!(
                    !out.contains(crate::i18n::MISSING_PREFIX),
                    "missing localization key for {lang} in release_detail render"
                );
            }
        }
    }
}
