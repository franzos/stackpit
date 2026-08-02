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
    /// `(org_id, label)` for the filter dropdown, alphabetical.
    orgs: Vec<(i64, String)>,
    sort: String,
    query: String,
    period: String,
    org: String,
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
    // `org` is deliberately not a browser default: the defaults cookie is only written
    // by the settings page, whose validator is DB-free and so cannot whitelist
    // arbitrary org ids. The filter is per-request until that gets a real home.
    if let Some(url) =
        defaults_redirect_url("/web/projects/", raw_qs.as_deref(), &defaults, &["period"])
    {
        return Ok(axum::response::Redirect::to(&url).into_response());
    }
    let sort_str = params.sort.clone().unwrap_or_default();
    let query_str = params.query.clone().unwrap_or_default();
    let period_str = params.period.clone().unwrap_or_else(|| "7d".to_string());
    let org_str = params.org.clone().unwrap_or_default();

    let since = period_to_timestamp(&period_str);

    // The unfiltered list backs the org dropdown; the name/id search is applied after.
    // Building the dropdown from the searched list would make it vanish whenever a
    // search narrowed to one org, stranding an active `org=` filter with no control.
    let all_projects = queries::projects::list_projects_cached(
        &pool,
        &state.project_list_cache,
        queries::projects::scope_for(&active_org),
        params.sort.as_deref().filter(|s| !s.is_empty()),
        None,
        since,
    )
    .await?;
    let orgs = org_options(&all_projects);

    let mut projects = all_projects;
    queries::projects::filter_projects_by_query(
        &mut projects,
        params.query.as_deref().filter(|s| !s.is_empty()),
    );

    // Org filter and org sort run over the cached list rather than the SQL, so they
    // add no cache-key dimensions.
    if let Ok(org_id) = org_str.parse::<i64>() {
        projects.retain(|p| p.org_id == org_id);
    }
    if sort_str == "org" {
        projects.sort_by(|a, b| {
            a.org_name
                .to_lowercase()
                .cmp(&b.org_name.to_lowercase())
                .then(b.project_id.cmp(&a.project_id))
        });
    }

    let tmpl = ProjectListTemplate {
        projects,
        orgs,
        sort: sort_str,
        query: query_str,
        period: period_str,
        org: org_str,
        chrome,
    };
    Ok(render_template(&tmpl))
}

/// Distinct orgs present in `projects`, alphabetical by label.
///
/// Dedupes by id before sorting: org names are not unique (only the slug is), so
/// deduping after a name sort would leave same-named orgs non-adjacent and emit
/// duplicate options.
fn org_options(projects: &[queries::ProjectSummary]) -> Vec<(i64, String)> {
    let mut orgs: Vec<(i64, String)> = projects
        .iter()
        .map(|p| (p.org_id, p.org_name.clone()))
        .collect();
    orgs.sort_unstable_by_key(|(id, _)| *id);
    orgs.dedup_by_key(|(id, _)| *id);
    orgs.sort_by(|a, b| {
        a.1.to_lowercase()
            .cmp(&b.1.to_lowercase())
            .then(a.0.cmp(&b.0))
    });
    orgs
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    fn tmpl_for(locale: &str) -> ProjectListTemplate {
        ProjectListTemplate {
            projects: Vec::new(),
            orgs: Vec::new(),
            sort: String::new(),
            query: String::new(),
            period: "7d".to_string(),
            org: String::new(),
            chrome: PageChrome::new(
                "test-csrf-token".into(),
                locale.parse().unwrap(),
                "/web/projects/".into(),
            ),
        }
    }

    fn project_in_org(project_id: u64, org_id: i64, org_name: &str) -> queries::ProjectSummary {
        queries::ProjectSummary {
            project_id,
            name: Some(format!("proj-{project_id}")),
            org_id,
            org_name: org_name.to_string(),
            archived: false,
            event_count: 0,
            error_count: 0,
            transaction_count: 0,
            session_count: 0,
            other_count: 0,
            issue_count: 0,
            first_seen: None,
            last_seen: None,
            platforms: String::new(),
            latest_release: None,
        }
    }

    // Pre-i18n-retrofit baseline: empty projects avoids timestamp filters so
    // the render is deterministic.
    #[test]
    fn project_list_renders_stable() {
        insta::assert_snapshot!(tmpl_for("en").render().unwrap());
    }

    // Proves the base.html chrome flip renders `lang="de" dir="ltr"` and that a
    // full base-extending page carries no missing localization keys in German.
    #[test]
    fn project_list_renders_german_without_missing_keys() {
        let html = tmpl_for("de").render().expect("project list renders");
        assert!(
            html.contains(r#"lang="de""#) && html.contains(r#"dir="ltr""#),
            "expected the German chrome language attributes in the output"
        );
        assert!(
            !html.contains(crate::i18n::MISSING_PREFIX),
            "German project-list render leaked a missing localization key: {html}"
        );
    }

    // Org names are not unique (only the slug is). Deduping by id after a name sort
    // would leave same-named orgs non-adjacent and emit the same org twice.
    #[test]
    fn org_options_dedupes_same_named_orgs_by_id() {
        let projects = vec![
            project_in_org(1, 10, "Acme"),
            project_in_org(2, 11, "Acme"),
            project_in_org(3, 10, "Acme"),
            project_in_org(4, 12, "Beta"),
        ];
        let orgs = org_options(&projects);
        assert_eq!(
            orgs,
            vec![
                (10, "Acme".to_string()),
                (11, "Acme".to_string()),
                (12, "Beta".to_string())
            ],
            "each org must appear exactly once"
        );
    }

    #[test]
    fn org_options_is_alphabetical_and_case_insensitive() {
        let projects = vec![
            project_in_org(1, 10, "zeta"),
            project_in_org(2, 11, "Alpha"),
        ];
        let orgs = org_options(&projects);
        assert_eq!(orgs[0].1, "Alpha");
        assert_eq!(orgs[1].1, "zeta");
    }

    // Every project carries its org, so a cross-org list is readable without
    // clicking through to find out where a project lives.
    #[test]
    fn org_column_renders_each_projects_org() {
        let mut tmpl = tmpl_for("en");
        tmpl.projects = vec![
            project_in_org(1, 10, "Acme GmbH"),
            project_in_org(2, 11, "Globex"),
        ];
        tmpl.orgs = vec![(10, "Acme GmbH".into()), (11, "Globex".into())];
        let html = tmpl.render().unwrap();

        assert!(html.contains("<bdi>Acme GmbH</bdi>"));
        assert!(html.contains("<bdi>Globex</bdi>"));
        assert!(html.contains("sort=org"), "org column must be sortable");
        assert!(
            html.contains(r#"<option value="10""#) && html.contains(r#"<option value="11""#),
            "filter dropdown must list both orgs: {html}"
        );
        assert!(!html.contains(crate::i18n::MISSING_PREFIX));
    }

    // A single-org install is the common self-hosted case; it should not grow a
    // dropdown with exactly one choice in it.
    #[test]
    fn org_filter_is_hidden_when_only_one_org() {
        let mut tmpl = tmpl_for("en");
        tmpl.projects = vec![project_in_org(1, 10, "Acme GmbH")];
        tmpl.orgs = vec![(10, "Acme GmbH".into())];
        let html = tmpl.render().unwrap();

        assert!(
            html.contains("<bdi>Acme GmbH</bdi>"),
            "column still renders"
        );
        assert!(
            !html.contains(r#"name="org""#),
            "no filter select for a single org: {html}"
        );
    }

    // The selected org must survive a sort click, or filtering then sorting silently
    // widens the list back to every org.
    #[test]
    fn sort_links_preserve_the_org_filter() {
        let mut tmpl = tmpl_for("en");
        tmpl.projects = vec![project_in_org(1, 10, "Acme GmbH")];
        tmpl.orgs = vec![(10, "Acme GmbH".into()), (11, "Globex".into())];
        tmpl.org = "10".to_string();
        let html = tmpl.render().unwrap();

        assert!(
            html.contains("sort=issues&amp;period=7d&amp;org=10"),
            "{html}"
        );
        assert!(html.contains(r#"<option value="10" selected"#), "{html}");
    }

    fn render_with_active_org(org: Option<String>) -> String {
        let mut tmpl = tmpl_for("en");
        tmpl.chrome = tmpl.chrome.with_active_org(org);
        tmpl.render().expect("project list renders")
    }

    // The active org is a mode with no other on-screen cue, so both indicators
    // (sidebar label and breadcrumb) have to carry the name.
    #[test]
    fn active_org_renders_in_both_indicators() {
        let html = render_with_active_org(Some("Acme GmbH".to_string()));
        assert!(
            html.contains(r#"aria-label="Active organization: Acme GmbH""#),
            "breadcrumb is missing the accessible scope label: {html}"
        );
        assert!(
            html.matches("<bdi>Acme GmbH</bdi>").count()
                + html
                    .matches(r#"<bdi class="text-on-surface">Acme GmbH</bdi>"#)
                    .count()
                >= 2,
            "expected the org name in both the sidebar and the breadcrumb: {html}"
        );
        assert!(!html.contains(crate::i18n::MISSING_PREFIX));
    }

    // Admin-token and loopback requests resolve an org id but no name; the
    // indicator must collapse rather than render an empty label.
    #[test]
    fn absent_active_org_renders_no_indicator() {
        let html = render_with_active_org(None);
        assert!(!html.contains("Active organization:"));
        assert!(!html.contains("Active:"));
    }

    // Proves the dir wiring end to end for an RTL locale (Arabic ships no
    // content, so this only asserts the chrome direction attributes).
    #[test]
    fn project_list_renders_rtl_dir_for_arabic() {
        let html = tmpl_for("ar").render().expect("project list renders");
        assert!(
            html.contains(r#"lang="ar""#) && html.contains(r#"dir="rtl""#),
            "expected the Arabic RTL chrome attributes in the output"
        );
    }
}
