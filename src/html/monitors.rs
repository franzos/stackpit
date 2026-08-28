use askama::Template;
use axum::extract::{Path, Query, State};

use crate::extractors::{ProjectPageCtx, ReadPool};
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::Chrome;
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::{PagedResult, Pagination};
use crate::queries::MonitorSummary;
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "monitors.html")]
struct MonitorListTemplate {
    project_id: u64,
    monitors: Vec<MonitorSummary>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn list_handler(ctx: ProjectPageCtx) -> Result<axum::response::Response, HtmlError> {
    let monitors = queries::monitors::list_monitors(&ctx.pool, ctx.project_id).await?;

    let tmpl = MonitorListTemplate {
        project_id: ctx.project_id,
        monitors,
        nav: ctx.nav,
        chrome: ctx.chrome,
    };
    Ok(render_template(&tmpl))
}

#[derive(Template)]
#[template(path = "monitor_detail.html")]
struct MonitorDetailTemplate {
    project_id: u64,
    slug: String,
    checkins: PagedResult<queries::EventSummary>,
    nav: queries::ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn detail_handler(
    active: ActiveOrg,
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    Path((project_id, slug)): Path<(u64, String)>,
    Query(params): Query<Pagination>,
) -> Result<axum::response::Response, HtmlError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(axum::http::StatusCode::NOT_FOUND, "Not found".into()))?;
    let page = params.page();
    let checkins =
        queries::monitors::list_checkins_for_monitor(&pool, project_id, &slug, &page).await?;

    let nav = state.nav_counts(project_id).await;

    let tmpl = MonitorDetailTemplate {
        project_id,
        slug,
        checkins,
        nav,
        chrome,
    };
    Ok(render_template(&tmpl))
}

#[cfg(test)]
mod i18n_tests {
    use super::*;
    use unic_langid::langid;

    fn empty_list(locale: crate::locale::LanguageIdentifier) -> MonitorListTemplate {
        MonitorListTemplate {
            project_id: 1,
            monitors: Vec::new(),
            nav: ProjectNavCounts::default(),
            chrome: PageChrome::new(String::new(), locale, "/web/projects/".into()),
        }
    }

    // Empty-collection render must not leak an unresolved Fluent key in either locale.
    #[test]
    fn monitor_list_renders_without_missing_keys() {
        for lang in [langid!("en"), langid!("de")] {
            let out = empty_list(lang.clone()).render().expect("render");
            assert!(
                !out.contains(crate::i18n::MISSING_PREFIX),
                "missing localization key for {lang} in monitor_list render"
            );
        }
    }

    // Empty renders skip the count-bearing paths, so exercise the cluster plurals directly.
    #[test]
    fn cluster_counted_keys_resolve() {
        for lang in [langid!("en"), langid!("de")] {
            let chrome = PageChrome::new(String::new(), lang.clone(), "/web/projects/".into());
            for (id, n) in [
                ("monitors-detail-count", 1),
                ("monitors-detail-count", 5),
                ("monitors-detail-confirm-delete-all", 5),
                ("monitors-detail-delete-all", 5),
                ("profiles-count", 5),
                ("replays-count", 5),
                ("releases-count", 5),
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

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    // Pre-i18n-retrofit baseline: empty check-ins avoid timestamp filters and
    // default nav counts keep the project sidebar deterministic.
    #[test]
    fn monitor_detail_renders_stable() {
        let tmpl = MonitorDetailTemplate {
            project_id: 42,
            slug: "nightly-backup".to_string(),
            checkins: PagedResult {
                items: Vec::new(),
                total: 0,
                offset: 0,
                limit: 25,
            },
            nav: ProjectNavCounts {
                label: "Test Project".to_string(),
                ..ProjectNavCounts::default()
            },
            chrome: PageChrome::new(
                "test-csrf-token".into(),
                crate::locale::default_locale(),
                "/web/projects/".into(),
            ),
        };
        insta::assert_snapshot!(tmpl.render().unwrap());
    }

    /// The all-matching affirmation has to sit above the table it acts on, as
    /// it does on the issue stream: below a screenful of rows it reads as a
    /// footnote to the last row rather than a gate on the whole filter.
    ///
    /// The snapshot above cannot catch this — its fixture has `total: 0`, so
    /// the gate never renders there.
    #[test]
    fn the_all_matching_gate_sits_above_the_table() {
        let tmpl = MonitorDetailTemplate {
            project_id: 42,
            slug: "nightly-backup".to_string(),
            checkins: PagedResult {
                items: vec![queries::EventSummary {
                    event_id: "e1".into(),
                    item_type: crate::ingest::models::ItemType::Event,
                    project_id: 42,
                    project_name: None,
                    fingerprint: Some("fp".into()),
                    timestamp: 1_700_000_000,
                    level: Some("info".into()),
                    title: Some("ok".into()),
                    platform: None,
                    release: None,
                    environment: None,
                }],
                total: 3,
                offset: 0,
                limit: 25,
            },
            nav: ProjectNavCounts::default(),
            chrome: PageChrome::new(
                "test-csrf-token".into(),
                crate::locale::default_locale(),
                "/web/projects/".into(),
            ),
        };
        let html = tmpl.render().unwrap();
        let gate = html.find("select-all-gate").expect("the gate renders");
        let table = html
            .find(r#"<table class="table">"#)
            .expect("the table renders");
        assert!(gate < table, "the gate must precede the table it governs");
    }
}
