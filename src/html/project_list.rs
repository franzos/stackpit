use askama::Template;
use axum::extract::{Query, RawQuery, State};
use axum::response::IntoResponse;

use crate::extractors::{BrowserDefaults, ReadPool};
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::{defaults_redirect_url, period_to_timestamp, Chrome, ListParams};
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::server::AppState;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "project_list.html")]
struct ProjectListTemplate {
    projects: Vec<queries::ProjectSummary>,
    sort: String,
    query: String,
    period: String,
    chrome: PageChrome,
}

pub async fn handler(
    BrowserDefaults(defaults): BrowserDefaults,
    RawQuery(raw_qs): RawQuery,
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    Query(params): Query<ListParams>,
    active_org: ActiveOrg,
) -> Result<axum::response::Response, HtmlError> {
    if let Some(url) =
        defaults_redirect_url("/web/projects/", raw_qs.as_deref(), &defaults, &["period"])
    {
        return Ok(axum::response::Redirect::to(&url).into_response());
    }
    let sort_str = params.sort.clone().unwrap_or_default();
    let query_str = params.query.clone().unwrap_or_default();
    let period_str = params.period.clone().unwrap_or_else(|| "7d".to_string());

    let since = period_to_timestamp(&period_str);

    let projects = queries::projects::list_projects_cached(
        &pool,
        &state.project_list_cache,
        active_org.org_id,
        params.sort.as_deref().filter(|s| !s.is_empty()),
        params.query.as_deref().filter(|s| !s.is_empty()),
        since,
    )
    .await?;

    let tmpl = ProjectListTemplate {
        projects,
        sort: sort_str,
        query: query_str,
        period: period_str,
        chrome,
    };
    Ok(render_template(&tmpl))
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    // Pre-i18n-retrofit baseline: empty projects avoids timestamp filters so
    // the render is deterministic.
    #[test]
    fn project_list_renders_stable() {
        let tmpl = ProjectListTemplate {
            projects: Vec::new(),
            sort: String::new(),
            query: String::new(),
            period: "7d".to_string(),
            chrome: PageChrome::new(
                "test-csrf-token".into(),
                crate::locale::default_locale(),
                "/web/projects/".into(),
            ),
        };
        insta::assert_snapshot!(tmpl.render().unwrap());
    }

    // Proves the base.html chrome flip renders `lang="de" dir="ltr"` and that a
    // full base-extending page carries no missing localization keys in German.
    #[test]
    fn project_list_renders_german_without_missing_keys() {
        let tmpl = ProjectListTemplate {
            projects: Vec::new(),
            sort: String::new(),
            query: String::new(),
            period: "7d".to_string(),
            chrome: PageChrome::new(
                "test-csrf-token".into(),
                "de".parse().unwrap(),
                "/web/projects/".into(),
            ),
        };
        let html = tmpl.render().expect("project list renders");
        assert!(
            html.contains(r#"lang="de""#) && html.contains(r#"dir="ltr""#),
            "expected the German chrome language attributes in the output"
        );
        assert!(
            !html.contains(crate::i18n::MISSING_PREFIX),
            "German project-list render leaked a missing localization key: {html}"
        );
    }

    // Proves the dir wiring end to end for an RTL locale (Arabic ships no
    // content, so this only asserts the chrome direction attributes).
    #[test]
    fn project_list_renders_rtl_dir_for_arabic() {
        let tmpl = ProjectListTemplate {
            projects: Vec::new(),
            sort: String::new(),
            query: String::new(),
            period: "7d".to_string(),
            chrome: PageChrome::new(
                "test-csrf-token".into(),
                "ar".parse().unwrap(),
                "/web/projects/".into(),
            ),
        };
        let html = tmpl.render().expect("project list renders");
        assert!(
            html.contains(r#"lang="ar""#) && html.contains(r#"dir="rtl""#),
            "expected the Arabic RTL chrome attributes in the output"
        );
    }
}
