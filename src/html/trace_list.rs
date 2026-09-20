use askama::Template;
use axum::extract::{Query, RawQuery, State};

use crate::extractors::{BrowserDefaults, ReadPool};
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::spans::{parse_project_filter, PROJECT_COLORS};
use crate::html::utils::{
    build_filter_qs, defaults_redirect, period_or_default, period_to_timestamp, Chrome, ListParams,
};
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::traces::TraceListFilter;
use crate::queries::types::PagedResult;
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

/// Chips past this many collapse into a counted "+n". Well past the number of
/// apps one trace realistically crosses.
const MAX_ROW_CHIPS: usize = 6;

/// One contributing project, as a chip that filters the list to itself.
pub struct TraceChip {
    pub project_id: i64,
    pub label: String,
    pub color: &'static str,
}

/// A trace, with everything the row renders pre-resolved.
pub struct TraceRow {
    pub trace_id: String,
    pub root_name: Option<String>,
    pub chips: Vec<TraceChip>,
    /// Contributing projects beyond [`MAX_ROW_CHIPS`].
    pub extra_projects: u64,
    pub transaction_count: u64,
    pub span_count: u64,
    pub error_count: u64,
    pub extent_ms: Option<i64>,
    pub last_timestamp: i64,
}

#[derive(Template)]
#[template(path = "traces_list.html")]
struct TraceListTemplate {
    result: PagedResult<TraceRow>,
    /// `?projects=` echoed back into the filter form.
    projects: String,
    /// `"1"` when the multi-project toggle is on, blank otherwise.
    multi: String,
    period: String,
    filter_qs: String,
    /// Everything but `projects`, so a chip can replace the project filter
    /// while keeping the period and the toggle.
    chip_qs: String,
    // Scoped to exactly one project (reached from its sidebar): keep the project
    // rail rather than dropping to the global one, as the release list does.
    project_nav: Option<ProjectNavCounts>,
    project_id_num: u64,
    chrome: PageChrome,
}

pub async fn handler(
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    BrowserDefaults(defaults): BrowserDefaults,
    RawQuery(raw_qs): RawQuery,
    Query(params): Query<ListParams>,
    active: ActiveOrg,
) -> Result<axum::response::Response, HtmlError> {
    if let Some(redirect) =
        defaults_redirect("/web/traces/", raw_qs.as_deref(), &defaults, &["period"])
    {
        return Ok(redirect);
    }
    let projects_str = params.projects.clone().unwrap_or_default();
    // An unchecked box submits nothing; echo the checked state back as "1" so
    // the pager carries it.
    let multi_on = matches!(params.multi.as_deref(), Some("1" | "on" | "true"));
    let multi_str = if multi_on { "1" } else { "" }.to_string();
    let period_str = period_or_default(params.period.as_deref());
    let since = period_to_timestamp(&period_str);

    let filter_ids = parse_project_filter(params.projects.as_deref());

    // The project rail only makes sense for a single project. With two or more
    // ids, or none, the page is org-level and uses `top_sidebar`.
    let rail_project = match filter_ids.as_deref() {
        Some([pid]) if *pid >= 0 => Some(*pid),
        _ => None,
    };
    let project_nav = match rail_project {
        Some(pid)
            if crate::orgs::extractor::require_project_scope(&active, &pool, pid)
                .await
                .is_ok() =>
        {
            Some(queries::projects::nav_counts_cached(&pool, &state.nav_cache, pid as u64).await)
        }
        _ => None,
    };

    // The read scope is never narrowed to the filtered project's org: `?projects=`
    // may name projects in two of the caller's orgs, and narrowing would silently
    // drop one. `?projects=` is a display filter; entitlement is the scope.
    let scope = crate::html::spans::trace_scope(&active);
    let filter = TraceListFilter {
        projects: filter_ids,
        multi_project_only: multi_on,
    };
    let page = params.page.page();
    let result = queries::traces::list_org_traces(&pool, &scope, &filter, &page, since).await?;

    let trace_ids: Vec<String> = result.items.iter().map(|t| t.trace_id.clone()).collect();
    let (rollups, span_counts) = tokio::join!(
        queries::traces::trace_event_rollups(&pool, &trace_ids, &scope, since),
        queries::traces::trace_span_counts(&pool, &trace_ids, &scope),
    );
    let rollups = rollups?;
    let span_counts = span_counts?;

    // Colour by first appearance down the page, so the colours are stable for a
    // given page rather than assigned per row. The order is also the id list
    // handed to `labels_for`: every id in it came back from a scoped read,
    // never from `?projects=`.
    let mut order: Vec<i64> = Vec::new();
    for trace in &result.items {
        if let Some(rollup) = rollups.get(&trace.trace_id) {
            for p in &rollup.projects {
                if !order.contains(&p.project_id) {
                    order.push(p.project_id);
                }
            }
        }
    }
    let labels = queries::projects::labels_for(&pool, &order)
        .await
        .unwrap_or_default();

    let items: Vec<TraceRow> = result
        .items
        .into_iter()
        .map(|trace| {
            let rollup = rollups.get(&trace.trace_id);
            let projects = rollup.map(|r| r.projects.as_slice()).unwrap_or_default();
            let chips = projects
                .iter()
                .take(MAX_ROW_CHIPS)
                .map(|p| TraceChip {
                    project_id: p.project_id,
                    label: labels
                        .get(&p.project_id)
                        .map(|l| l.label.clone())
                        .unwrap_or_else(|| format!("Project {}", p.project_id)),
                    color: PROJECT_COLORS[order
                        .iter()
                        .position(|&o| o == p.project_id)
                        .unwrap_or(0)
                        % PROJECT_COLORS.len()],
                })
                .collect();
            TraceRow {
                chips,
                extra_projects: projects.len().saturating_sub(MAX_ROW_CHIPS) as u64,
                span_count: span_counts
                    .get(&trace.trace_id)
                    .copied()
                    .unwrap_or_default(),
                error_count: rollup.map(|r| r.error_count).unwrap_or_default(),
                transaction_count: trace.transaction_count,
                extent_ms: trace.extent_ms,
                last_timestamp: trace.last_timestamp,
                root_name: trace.root_name,
                trace_id: trace.trace_id,
            }
        })
        .collect();

    // No column sorting on this page, so only the pager's filter string is used.
    let (_, filter_qs) = build_filter_qs(
        &[
            ("projects", &projects_str),
            ("multi", &multi_str),
            ("period", &period_str),
        ],
        "",
    );
    let (chip_qs, _) = build_filter_qs(&[("multi", &multi_str), ("period", &period_str)], "");

    let tmpl = TraceListTemplate {
        result: PagedResult {
            items,
            total: result.total,
            offset: result.offset,
            limit: result.limit,
        },
        projects: projects_str,
        multi: multi_str,
        period: period_str,
        filter_qs,
        chip_qs,
        project_nav,
        project_id_num: rail_project.unwrap_or(0) as u64,
        chrome,
    };

    Ok(render_template(&tmpl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::LanguageIdentifier;
    use crate::orgs::Role;
    use unic_langid::langid;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn chrome() -> Chrome {
        Chrome(PageChrome::new(
            "csrf".into(),
            langid!("en"),
            "/web/traces/".into(),
        ))
    }

    fn member_of(orgs: &[i64]) -> ActiveOrg {
        ActiveOrg::with_memberships(
            orgs[0],
            Some(Role::Member),
            orgs.iter().map(|&o| (o, Role::Member)).collect(),
        )
    }

    async fn body(resp: axum::response::Response) -> String {
        use axum::body::to_bytes;
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Projects 10, 11 in org 20; project 12 in org 21. Trace A spans 10 and 11,
    /// trace B is project 10 alone.
    async fn seeded() -> crate::db::DbPool {
        let pool = crate::db::open_test_pool().await;
        for (org_id, slug) in [(20i64, "alpha"), (21, "beta")] {
            sqlx::query(crate::db::sql!(
                "INSERT INTO organizations (org_id, slug, name) VALUES (?1, ?2, ?2)"
            ))
            .bind(org_id)
            .bind(slug)
            .execute(&pool)
            .await
            .unwrap();
        }
        for (project_id, org_id, name) in [(10i64, 20i64, "kyc"), (11, 20, "idp"), (12, 21, "far")]
        {
            sqlx::query(crate::db::sql!(
                "INSERT INTO projects (project_id, org_id, name) VALUES (?1, ?2, ?3)"
            ))
            .bind(project_id)
            .bind(org_id)
            .bind(name)
            .execute(&pool)
            .await
            .unwrap();
        }

        let blob = zstd::encode_all([0u8; 0].as_slice(), 3).unwrap();
        for (event_id, project_id, trace, name, ts) in [
            ("e1", 10i64, A, "POST /kyc/start", 1_700_000_000i64),
            ("e2", 11, A, "POST /users/registration", 1_700_000_001),
            ("e3", 10, B, "GET /health", 1_700_000_002),
            (
                "e4",
                12,
                "cccccccccccccccccccccccccccccccc",
                "GET /far",
                1_700_000_003,
            ),
        ] {
            sqlx::query(crate::db::sql!(
                "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp,
                                     transaction_name, trace_id, duration_ms, start_ms)
                 VALUES (?1, 'transaction', ?2, ?3, 'k', ?4, ?5, ?6, 100, 1700000000000)"
            ))
            .bind(event_id)
            .bind(&blob)
            .bind(project_id)
            .bind(ts)
            .bind(name)
            .bind(trace)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool
    }

    async fn page(
        pool: &crate::db::DbPool,
        active: ActiveOrg,
        qs: &str,
    ) -> Result<axum::response::Response, HtmlError> {
        let (state, _chans) = crate::server::AppState::for_test(pool.clone());
        let params: ListParams = serde_urlencoded::from_str(qs).unwrap();
        handler(
            State(state),
            ReadPool(pool.clone()),
            chrome(),
            BrowserDefaults(std::collections::HashMap::new()),
            RawQuery(Some(qs.to_string())),
            Query(params),
            active,
        )
        .await
    }

    #[tokio::test]
    async fn a_member_sees_only_their_own_projects_traces() {
        let pool = seeded().await;
        let html = body(page(&pool, member_of(&[20]), "period=").await.unwrap()).await;
        assert!(html.contains("POST /kyc/start"));
        assert!(html.contains("GET /health"));
        assert!(!html.contains("GET /far"), "org 21's trace leaked");
        assert!(
            !html.contains("far"),
            "even the project label must not leak"
        );
        // Both contributing projects are chipped on the cross-project trace.
        assert!(html.contains("kyc") && html.contains("idp"));
    }

    #[tokio::test]
    async fn the_projects_filter_narrows_without_leaking_an_out_of_scope_label() {
        let pool = seeded().await;
        let html = body(
            page(&pool, member_of(&[20]), "projects=11&period=")
                .await
                .unwrap(),
        )
        .await;
        assert!(
            html.contains("POST /kyc/start"),
            "trace A includes project 11"
        );
        assert!(
            !html.contains("GET /health"),
            "trace B has no project 11 hop"
        );

        // An id outside the caller's scope matches no trace and yields no label.
        let foreign = body(
            page(&pool, member_of(&[20]), "projects=12&period=")
                .await
                .unwrap(),
        )
        .await;
        assert!(!foreign.contains("GET /far"));
        assert!(!foreign.contains("far"));
        assert!(!foreign.contains("POST /kyc/start"));
    }

    #[tokio::test]
    async fn the_multi_toggle_drops_single_project_traces() {
        let pool = seeded().await;
        let html = body(
            page(&pool, member_of(&[20]), "multi=1&period=")
                .await
                .unwrap(),
        )
        .await;
        assert!(html.contains("POST /kyc/start"));
        assert!(
            !html.contains("GET /health"),
            "single-project trace dropped"
        );
    }

    // The pager only renders past the page size, so this has to page for real:
    // a toggle the next-page link drops silently re-widens the list.
    #[tokio::test]
    async fn the_filters_survive_paging() {
        let pool = seeded().await;
        // Two traces touch project 10, so `limit=1` forces a second page.
        let html = body(
            page(&pool, member_of(&[20]), "limit=1&period=&projects=10")
                .await
                .unwrap(),
        )
        .await;
        let next = html
            .split("href=\"")
            .find(|s| s.starts_with("/web/traces/?offset="))
            .expect("a next-page link is rendered");
        assert!(next.contains("projects=10"), "{next}");
        assert!(next.contains("period="), "{next}");

        // The toggle rides the same pager. Without `?projects=` both traces are
        // still there, so the page splits the same way.
        let toggled = body(
            page(
                &pool,
                member_of(&[20]),
                "limit=1&period=&multi=1&projects=10,11",
            )
            .await
            .unwrap(),
        )
        .await;
        assert!(
            !toggled.contains("/web/traces/?offset="),
            "the toggle leaves one row, which fits on one page"
        );
        let unfiltered = body(
            page(&pool, member_of(&[20]), "limit=1&period=")
                .await
                .unwrap(),
        )
        .await;
        let next = unfiltered
            .split("href=\"")
            .find(|s| s.starts_with("/web/traces/?offset="))
            .expect("a next-page link is rendered");
        assert!(next.contains("period="), "{next}");
    }

    // Two orgs plus a `?projects=` naming a project in each: the read scope must
    // stay every membership. Narrowing it to one project's org drops the other.
    #[tokio::test]
    async fn a_filter_spanning_two_orgs_reads_both() {
        let pool = seeded().await;
        let html = body(
            page(&pool, member_of(&[20, 21]), "projects=10,12&period=")
                .await
                .unwrap(),
        )
        .await;
        assert!(html.contains("POST /kyc/start"), "org 20's trace");
        assert!(html.contains("GET /far"), "org 21's trace");
    }

    #[tokio::test]
    async fn the_project_rail_needs_exactly_one_id() {
        let pool = seeded().await;
        let one = body(
            page(&pool, member_of(&[20]), "projects=10&period=")
                .await
                .unwrap(),
        )
        .await;
        assert!(
            one.contains("/web/projects/10/transactions/"),
            "a single id keeps the project rail"
        );

        let several = body(
            page(&pool, member_of(&[20]), "projects=10,11&period=")
                .await
                .unwrap(),
        )
        .await;
        assert!(
            !several.contains("/web/projects/10/transactions/"),
            "two ids are org-level: top_sidebar, no rail"
        );
    }

    #[tokio::test]
    async fn defaults_redirect_fires_once_for_period() {
        let pool = seeded().await;
        let (state, _chans) = crate::server::AppState::for_test(pool.clone());
        let mut defaults = std::collections::HashMap::new();
        defaults.insert("period".to_string(), "30d".to_string());
        let resp = handler(
            State(state),
            ReadPool(pool.clone()),
            chrome(),
            BrowserDefaults(defaults),
            RawQuery(None),
            Query(serde_urlencoded::from_str("").unwrap()),
            member_of(&[20]),
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::SEE_OTHER);
        let location = resp.headers()["location"].to_str().unwrap().to_string();
        assert_eq!(location, "/web/traces/?period=30d");

        // Following it does not redirect again.
        let again = page(&pool, member_of(&[20]), "period=30d").await.unwrap();
        assert_eq!(again.status(), axum::http::StatusCode::OK);
    }

    fn empty_template(locale: LanguageIdentifier) -> TraceListTemplate {
        TraceListTemplate {
            result: PagedResult {
                items: Vec::new(),
                total: 0,
                offset: 0,
                limit: 25,
            },
            projects: String::new(),
            multi: String::new(),
            period: String::new(),
            filter_qs: String::new(),
            chip_qs: String::new(),
            project_nav: None,
            project_id_num: 0,
            chrome: PageChrome::new(String::new(), locale, "/web/traces/".into()),
        }
    }

    // Empty-collection render must not leak an unresolved Fluent key in either locale.
    #[test]
    fn renders_without_missing_keys() {
        for lang in [langid!("en"), langid!("de")] {
            let out = empty_template(lang.clone()).render().expect("render");
            assert!(
                !out.contains(crate::i18n::MISSING_PREFIX),
                "missing localization key for {lang} in trace_list render"
            );
        }
    }

    // The empty render skips the count-bearing strings, so exercise those directly.
    #[test]
    fn counted_keys_resolve() {
        for lang in [langid!("en"), langid!("de")] {
            let chrome = PageChrome::new(String::new(), lang.clone(), "/web/traces/".into());
            for (id, n) in [
                ("traces-count", 1),
                ("traces-count", 5),
                ("traces-more-projects", 1),
                ("traces-more-projects", 3),
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
