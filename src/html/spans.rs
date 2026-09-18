use askama::Template;
use axum::extract::{Path, Query, RawQuery, State};

use crate::extractors::{BrowserDefaults, ProjectPageCtx, ReadPool};
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::{
    cross_org_scope, defaults_redirect, period_or_default, period_to_timestamp, Chrome,
    CrossOrgScope, ListParams,
};
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::{
    PagedResult, SpanAggregation, SpanSummary, TraceError, TraceSummary, Waterfall,
};
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "span_list.html")]
struct SpanListTemplate {
    project_id: u64,
    result: PagedResult<SpanSummary>,
    traces: PagedResult<TraceSummary>,
    aggregates: SpanAggregation,
    agg_cap: usize,
    period: String,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

#[derive(Template)]
#[template(path = "trace_detail.html")]
struct TraceDetailTemplate {
    project_id: u64,
    trace_id: String,
    view: TraceView,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

#[derive(Template)]
#[template(path = "org_trace_detail.html")]
struct OrgTraceDetailTemplate {
    trace_id: String,
    /// Whether `?projects=` is narrowing the view, for the "show all" affordance.
    filtered: bool,
    view: TraceView,
    chrome: PageChrome,
}

pub async fn list_handler(
    ctx: ProjectPageCtx,
    BrowserDefaults(defaults): BrowserDefaults,
    RawQuery(raw_qs): RawQuery,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    if let Some(redirect) = defaults_redirect(
        &format!("/web/projects/{}/spans/", ctx.project_id),
        raw_qs.as_deref(),
        &defaults,
        &["period"],
    ) {
        return Ok(redirect);
    }
    let page = params.page.page();
    let trace_page = params.trace_page.page();
    let period = period_or_default(params.period.as_deref());
    let since = period_to_timestamp(&period);

    let (span_result, trace_result, agg_result) = tokio::join!(
        queries::spans::list_spans(&ctx.pool, ctx.project_id, &page, since),
        queries::spans::list_traces(&ctx.pool, ctx.project_id, &trace_page, since),
        queries::spans::aggregate_spans(&ctx.pool, ctx.project_id, since),
    );

    let result = span_result?;
    let traces = trace_result?;
    let aggregates = agg_result?;

    let tmpl = SpanListTemplate {
        project_id: ctx.project_id,
        result,
        traces,
        aggregates,
        agg_cap: queries::spans::MAX_SPAN_GROUPS,
        period,
        nav: ctx.nav,
        chrome: ctx.chrome,
    };
    Ok(render_template(&tmpl))
}

/// One colour per contributing project, assigned by first appearance. Eight is
/// well past the number of apps a single trace realistically crosses; beyond
/// that the palette repeats rather than growing unreadable.
const PROJECT_COLORS: [&str; 8] = [
    "#2563eb", "#db2777", "#16a34a", "#d97706", "#7c3aed", "#0891b2", "#be123c", "#65a30d",
];

/// `?projects=1,2` — the saved view. Nothing is stored server-side.
#[derive(serde::Deserialize, Default)]
pub struct TraceViewParams {
    projects: Option<String>,
}

/// More projects than any real trace crosses. Bounds the per-row membership
/// test so a long `?projects=` list can't turn one page load into a scan.
const MAX_FILTER_PROJECTS: usize = 64;

impl TraceViewParams {
    /// Project ids to render, or `None` for the full view. Malformed entries are
    /// dropped; ids outside the caller's scope simply match no rows.
    fn project_filter(&self) -> Option<Vec<i64>> {
        let raw = self.projects.as_deref()?.trim();
        if raw.is_empty() {
            return None;
        }
        let mut ids: Vec<i64> = raw
            .split(',')
            .filter_map(|s| s.trim().parse::<i64>().ok())
            .take(MAX_FILTER_PROJECTS)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        (!ids.is_empty()).then_some(ids)
    }
}

/// The one place a caller's entitlement becomes a trace read scope. Both trace
/// handlers go through it so the superuser and membership cases cannot drift
/// apart: a caller with a role never reaches `All`.
fn trace_scope(
    active: &ActiveOrg,
    project_scope: Option<&crate::orgs::extractor::ProjectScope>,
) -> queries::spans::TraceScope {
    match cross_org_scope(active, project_scope) {
        CrossOrgScope::All => queries::spans::TraceScope::All,
        CrossOrgScope::Project(org_id) => queries::spans::TraceScope::Orgs(vec![org_id]),
        CrossOrgScope::Memberships(ids) => queries::spans::TraceScope::Orgs(ids),
    }
}

/// One project contributing to the trace, with its toggle link.
pub struct LegendEntry {
    pub project_id: i64,
    pub label: String,
    /// Only set when the trace spans more than one org, so the boundary is visible.
    pub org_name: Option<String>,
    pub color: &'static str,
    pub transaction_count: usize,
    pub selected: bool,
    /// Query string this entry's link navigates to, `?projects=...` or empty.
    pub toggle_qs: String,
}

/// "n more transactions in m other projects", on the per-project page.
pub struct OtherProjectsBanner {
    pub transactions: usize,
    pub projects: usize,
}

/// A waterfall row with everything the template needs pre-resolved.
pub struct TraceRow {
    pub row: crate::queries::types::WaterfallRow,
    pub project_label: String,
    pub color: &'static str,
    /// Parent reported but not rendered: filtered out, unreadable, or never
    /// reported. Deliberately never says which, nor names the missing project.
    pub parent_missing: bool,
    /// Event id of the first error on this row, for the error-dot link.
    pub first_error: Option<String>,
}

/// An error row with its project resolved for the badge and the link.
pub struct TraceErrorRow {
    pub err: TraceError,
    pub project_label: String,
    pub color: &'static str,
}

/// Everything the trace page renders, at one scope and one visible project set.
pub struct TraceView {
    pub waterfall: Waterfall,
    pub rows: Vec<TraceRow>,
    pub errors: Vec<TraceErrorRow>,
    pub legend: Vec<LegendEntry>,
    pub span_total: usize,
    pub span_shown: usize,
    pub other_projects: Option<OtherProjectsBanner>,
}

/// Read a trace at `scope` and render the rows in `visible` (all of them when
/// `None`). The scope is the security boundary; `visible` is only what is drawn,
/// which is why the banner can count transactions the page does not show.
///
/// Rows are filtered *before* the waterfall is built, so a row whose parent was
/// filtered out surfaces as a root carrying the "parent not in view" marker
/// rather than an orphan indented under nothing.
pub async fn build_trace_view(
    pool: &crate::db::DbPool,
    trace_id: &str,
    scope: &queries::spans::TraceScope,
    visible: Option<&[i64]>,
) -> Result<TraceView, HtmlError> {
    let (spans, transactions, errors) = tokio::join!(
        queries::spans::get_trace_spans(pool, trace_id, scope),
        queries::spans::get_trace_transactions(pool, trace_id, scope),
        queries::spans::get_trace_errors(pool, trace_id, scope),
    );
    let spans = spans?;
    let transactions = transactions?;
    let errors = errors?;

    let visible_set: Option<std::collections::HashSet<i64>> =
        visible.map(|v| v.iter().copied().collect());
    let shown = |project_id: i64| visible_set.as_ref().is_none_or(|v| v.contains(&project_id));

    // The legend describes the whole trace at this scope, not the filtered view,
    // so a project toggled off can be toggled back on. Ordered by first
    // appearance on the timeline, with the id as tiebreak, so colours stay put
    // across reloads even when two hops share a second.
    let mut first_seen: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    let mut txn_counts: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    let mut note = |project_id: i64, start_ms: Option<i64>| {
        let at = start_ms.unwrap_or(i64::MAX);
        first_seen
            .entry(project_id)
            .and_modify(|e| *e = (*e).min(at))
            .or_insert(at);
    };
    for t in &transactions {
        note(t.project_id, t.start_ms);
        *txn_counts.entry(t.project_id).or_default() += 1;
    }
    for s in &spans {
        note(s.project_id, s.start_ms);
    }
    let mut order: Vec<i64> = first_seen.keys().copied().collect();
    order.sort_by_key(|p| (first_seen[p], *p));

    let labels = queries::projects::labels_for(pool, &order)
        .await
        .unwrap_or_default();
    let label_of = |project_id: i64| {
        labels
            .get(&project_id)
            .map(|l| l.label.clone())
            .unwrap_or_else(|| format!("Project {project_id}"))
    };
    let color_of = |project_id: i64| {
        let idx = order.iter().position(|&p| p == project_id).unwrap_or(0);
        PROJECT_COLORS[idx % PROJECT_COLORS.len()]
    };

    let banner = visible_set.as_ref().map(|_| {
        let others: std::collections::HashSet<i64> = transactions
            .iter()
            .map(|t| t.project_id)
            .filter(|p| !shown(*p))
            .collect();
        OtherProjectsBanner {
            transactions: transactions.iter().filter(|t| !shown(t.project_id)).count(),
            projects: others.len(),
        }
    });
    let other_projects = banner.filter(|b| b.transactions > 0);

    let mut span_rows: Vec<queries::spans::SpanRow> = transactions
        .iter()
        .filter(|t| shown(t.project_id))
        .map(Into::into)
        .collect();
    span_rows.extend(
        spans
            .iter()
            .filter(|s| shown(s.project_id))
            .map(queries::spans::SpanRow::from),
    );

    let errors: Vec<TraceError> = errors.into_iter().filter(|e| shown(e.project_id)).collect();

    // The web path takes its extent from the rows themselves; `root_duration_ms`
    // stays for the MCP path, which still draws a separate root row.
    let mut waterfall = queries::spans::build_waterfall(&span_rows, 0);
    queries::spans::attach_error_counts(&mut waterfall.rows, &errors);

    let span_total = waterfall.span_count;
    let span_shown = waterfall.rows.len();

    let rendered: std::collections::HashSet<&str> =
        waterfall.rows.iter().map(|r| r.span_id.as_str()).collect();
    let parent_missing: Vec<bool> = waterfall
        .rows
        .iter()
        .map(|r| {
            r.parent_span_id
                .as_deref()
                .is_some_and(|p| !rendered.contains(p))
        })
        .collect();

    let rows: Vec<TraceRow> = std::mem::take(&mut waterfall.rows)
        .into_iter()
        .zip(parent_missing)
        .map(|(row, parent_missing)| TraceRow {
            first_error: (row.error_count > 0)
                .then(|| {
                    errors
                        .iter()
                        .find(|e| e.span_id.as_deref() == Some(row.span_id.as_str()))
                        .map(|e| e.event_id.clone())
                })
                .flatten(),
            project_label: label_of(row.project_id),
            color: color_of(row.project_id),
            parent_missing,
            row,
        })
        .collect();

    let multi_org = {
        let orgs: std::collections::HashSet<i64> = order
            .iter()
            .filter_map(|p| labels.get(p))
            .map(|l| l.org_id)
            .collect();
        orgs.len() > 1
    };
    let selected: Vec<i64> = visible
        .map(<[i64]>::to_vec)
        .unwrap_or_else(|| order.clone());
    let legend: Vec<LegendEntry> = order
        .iter()
        .map(|&project_id| LegendEntry {
            project_id,
            label: label_of(project_id),
            org_name: multi_org.then(|| {
                labels
                    .get(&project_id)
                    .map(|l| l.org_name.clone())
                    .unwrap_or_default()
            }),
            color: color_of(project_id),
            transaction_count: txn_counts.get(&project_id).copied().unwrap_or_default(),
            selected: selected.contains(&project_id),
            toggle_qs: toggle_query(&order, &selected, project_id),
        })
        .collect();

    Ok(TraceView {
        waterfall,
        rows,
        errors: errors
            .into_iter()
            .map(|err| TraceErrorRow {
                project_label: label_of(err.project_id),
                color: color_of(err.project_id),
                err,
            })
            .collect(),
        legend,
        span_total,
        span_shown,
        other_projects,
    })
}

/// Query string for flipping one project in the legend. Selecting everything,
/// or nothing, both collapse to the clean unfiltered URL.
fn toggle_query(all: &[i64], selected: &[i64], project_id: i64) -> String {
    let mut next: Vec<i64> = all
        .iter()
        .copied()
        .filter(|p| (selected.contains(p)) != (*p == project_id))
        .collect();
    next.sort_unstable();
    if next.is_empty() || next.len() == all.len() {
        return String::new();
    }
    let ids: Vec<String> = next.iter().map(i64::to_string).collect();
    format!("?projects={}", ids.join(","))
}

/// Full 32-hex or nothing: a partial id is a search term, not a trace page.
fn full_trace_id(raw: &str) -> Option<String> {
    queries::trace_id_candidate(raw).filter(|t| t.len() == queries::TRACE_ID_LEN)
}

fn not_found() -> HtmlError {
    HtmlError(axum::http::StatusCode::NOT_FOUND, "Not found".into())
}

pub async fn trace_detail_handler(
    active: ActiveOrg,
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    Path((project_id, trace_id)): Path<(u64, String)>,
) -> Result<axum::response::Response, HtmlError> {
    let trace_id = full_trace_id(&trace_id).ok_or_else(not_found)?;
    let project_scope =
        crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
            .await
            .map_err(|_| not_found())?;

    // Read at the owning org's scope so the banner can count what other readable
    // projects contributed, then render only this project's rows.
    let scope = trace_scope(&active, Some(&project_scope));
    let view = build_trace_view(&pool, &trace_id, &scope, Some(&[project_id as i64])).await?;

    let nav = state.nav_counts(project_id).await;

    let tmpl = TraceDetailTemplate {
        project_id,
        trace_id,
        view,
        nav,
        chrome,
    };
    Ok(render_template(&tmpl))
}

/// The org-level trace page: every project the caller can read, on one
/// waterfall. No project selector — the scope is who the caller is.
pub async fn org_trace_detail_handler(
    active: ActiveOrg,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    Path(trace_id): Path<String>,
    Query(params): Query<TraceViewParams>,
) -> Result<axum::response::Response, HtmlError> {
    let trace_id = full_trace_id(&trace_id).ok_or_else(not_found)?;
    let scope = trace_scope(&active, None);
    let filter = params.project_filter();
    let view = build_trace_view(&pool, &trace_id, &scope, filter.as_deref()).await?;
    // Errors count as rows: below a sampling rate of 1 a trace routinely has
    // error events and no sampled transaction, and the panel is the whole point
    // of looking it up. Only a trace with nothing at all is a 404.
    if view.span_total == 0 && view.legend.is_empty() && view.errors.is_empty() {
        return Err(not_found());
    }

    let tmpl = OrgTraceDetailTemplate {
        trace_id,
        filtered: filter.is_some(),
        view,
        chrome,
    };
    Ok(render_template(&tmpl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orgs::Role;
    use unic_langid::langid;

    const TRACE: &str = "aabbccddeeff00112233445566778899";

    fn chrome() -> Chrome {
        Chrome(PageChrome::new(
            "csrf".into(),
            langid!("en"),
            "/web/projects/".into(),
        ))
    }

    fn member_of(orgs: &[i64]) -> ActiveOrg {
        ActiveOrg::with_memberships(
            orgs[0],
            Some(Role::Member),
            orgs.iter().map(|&o| (o, Role::Member)).collect(),
        )
    }

    fn superuser() -> ActiveOrg {
        ActiveOrg::with_memberships(1, None, Vec::new())
    }

    async fn body(resp: axum::response::Response) -> String {
        use axum::body::to_bytes;
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Projects 10 and 11 in org 20, project 12 in org 21. One transaction each
    /// on the same trace; 11's hangs off the `http.client` span 10 opened.
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
        let txn = |event_id: &'static str,
                   project_id: i64,
                   name: &'static str,
                   span_id: &'static str,
                   parent: Option<&'static str>| {
            let pool = pool.clone();
            let blob = blob.clone();
            async move {
                sqlx::query(crate::db::sql!(
                    "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp,
                                         transaction_name, trace_id, duration_ms, span_id, parent_span_id, start_ms)
                     VALUES (?1, 'transaction', ?2, ?3, 'k', 1700000000, ?4, ?5, 100, ?6, ?7, 1700000000000)"
                ))
                .bind(event_id)
                .bind(&blob)
                .bind(project_id)
                .bind(name)
                .bind(TRACE)
                .bind(span_id)
                .bind(parent)
                .execute(&pool)
                .await
                .unwrap();
            }
        };
        txn("e10", 10, "POST /kyc/start", "a000", None).await;
        txn(
            "e11",
            11,
            "POST /users/registration",
            "b000",
            Some("a-http"),
        )
        .await;
        txn("e12", 12, "GET /far", "c000", None).await;

        // Project 10's outbound client span, the stitch point for project 11.
        sqlx::query(crate::db::sql!(
            "INSERT INTO spans (span_id, payload, project_id, public_key, timestamp, trace_id,
                                parent_span_id, op, description, status, duration_ms, start_ms)
             VALUES ('a-http', ?1, 10, 'k', 1700000000, ?2, 'a000', 'http.client', 'POST http://idp/', 'ok', 80, 1700000000010)"
        ))
        .bind(&blob)
        .bind(TRACE)
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    async fn org_page(
        pool: &crate::db::DbPool,
        active: ActiveOrg,
        trace_id: &str,
        projects: Option<&str>,
    ) -> Result<axum::response::Response, HtmlError> {
        org_trace_detail_handler(
            active,
            ReadPool(pool.clone()),
            chrome(),
            Path(trace_id.to_string()),
            Query(TraceViewParams {
                projects: projects.map(String::from),
            }),
        )
        .await
    }

    async fn project_page(
        pool: &crate::db::DbPool,
        active: ActiveOrg,
        project_id: u64,
    ) -> Result<axum::response::Response, HtmlError> {
        let (state, _chans) = crate::server::AppState::for_test(pool.clone());
        trace_detail_handler(
            active,
            State(state),
            ReadPool(pool.clone()),
            chrome(),
            Path((project_id, TRACE.to_string())),
        )
        .await
    }

    // A caller with a role must never reach `All`, whichever page they land on.
    #[test]
    fn only_a_superuser_reaches_the_unscoped_trace_read() {
        assert_eq!(
            trace_scope(&superuser(), None),
            queries::spans::TraceScope::All
        );
        assert_eq!(
            trace_scope(&member_of(&[20, 21]), None),
            queries::spans::TraceScope::Orgs(vec![20, 21])
        );
        let scope = crate::orgs::extractor::ProjectScope {
            org_id: 20,
            role: Some(Role::Member),
        };
        assert_eq!(
            trace_scope(&member_of(&[20]), Some(&scope)),
            queries::spans::TraceScope::Orgs(vec![20])
        );
    }

    #[test]
    fn the_project_filter_drops_junk_and_bounds_itself() {
        let parse = |s: &str| {
            TraceViewParams {
                projects: Some(s.into()),
            }
            .project_filter()
        };
        assert_eq!(parse("10,11"), Some(vec![10, 11]));
        assert_eq!(
            parse(" 11 , 10 ,"),
            Some(vec![10, 11]),
            "sorted and trimmed"
        );
        assert_eq!(parse("10,10,10"), Some(vec![10]), "deduped");
        assert_eq!(parse("abc"), None, "no parseable id is no filter");
        assert_eq!(parse(""), None);
        assert_eq!(TraceViewParams::default().project_filter(), None);

        let many: Vec<String> = (0..500).map(|i| i.to_string()).collect();
        assert_eq!(
            parse(&many.join(",")).unwrap().len(),
            super::MAX_FILTER_PROJECTS
        );
    }

    #[tokio::test]
    async fn org_page_404s_on_a_malformed_or_unknown_trace_id() {
        let pool = seeded().await;
        for bad in ["not-hex-at-all", "deadbeef", "", &TRACE[..31]] {
            assert!(
                org_page(&pool, superuser(), bad, None).await.is_err(),
                "{bad} must not resolve to a trace page"
            );
        }
        assert!(
            org_page(&pool, superuser(), &"f".repeat(32), None)
                .await
                .is_err(),
            "a well-formed id with no rows is a 404, not an empty page"
        );
    }

    // Below a sampling rate of 1 a trace often has errors and no sampled
    // transaction. 404ing there would hide the only thing worth looking at.
    #[tokio::test]
    async fn a_trace_with_only_errors_still_renders_its_panel() {
        let pool = seeded().await;
        let errors_only = "ffffffffffffffffffffffffffffffff";
        sqlx::query(crate::db::sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp,
                                 title, level, trace_id, span_id)
             VALUES ('lonely', 'event', ?1, 10, 'k', 1700000000, 'DuplicateEmail', 'error', ?2, 'gone')"
        ))
        .bind(zstd::encode_all([0u8; 0].as_slice(), 3).unwrap())
        .bind(errors_only)
        .execute(&pool)
        .await
        .unwrap();

        let html = body(
            org_page(&pool, member_of(&[20]), errors_only, None)
                .await
                .expect("a trace with errors and no spans is not a 404"),
        )
        .await;
        assert!(html.contains("DuplicateEmail"));
        assert!(html.contains("Correlated errors"));
    }

    #[tokio::test]
    async fn a_member_sees_their_orgs_projects_and_never_a_foreign_one() {
        let pool = seeded().await;
        let html = body(
            org_page(&pool, member_of(&[20]), TRACE, None)
                .await
                .unwrap(),
        )
        .await;
        assert!(html.contains("POST /kyc/start"));
        assert!(html.contains("POST /users/registration"));
        assert!(!html.contains("GET /far"), "org 21's transaction leaked");
        assert!(
            !html.contains("far"),
            "even the project label must not leak"
        );
    }

    #[tokio::test]
    async fn a_member_of_two_orgs_sees_both_with_org_names_in_the_legend() {
        let pool = seeded().await;
        let html = body(
            org_page(&pool, member_of(&[20, 21]), TRACE, None)
                .await
                .unwrap(),
        )
        .await;
        assert!(html.contains("GET /far"));
        assert!(html.contains("alpha") && html.contains("beta"), "org names");

        // One org contributing: the legend stays project-only.
        let single = body(
            org_page(&pool, member_of(&[20]), TRACE, None)
                .await
                .unwrap(),
        )
        .await;
        assert!(
            !single.contains("alpha"),
            "org names only when the trace crosses orgs"
        );
    }

    #[tokio::test]
    async fn superuser_sees_every_project() {
        let pool = seeded().await;
        let html = body(org_page(&pool, superuser(), TRACE, None).await.unwrap()).await;
        for name in ["POST /kyc/start", "POST /users/registration", "GET /far"] {
            assert!(html.contains(name), "missing {name}");
        }
    }

    #[tokio::test]
    async fn the_filter_narrows_and_ignores_ids_outside_the_scope() {
        let pool = seeded().await;
        let html = body(
            org_page(&pool, member_of(&[20]), TRACE, Some("11"))
                .await
                .unwrap(),
        )
        .await;
        assert!(html.contains("POST /users/registration"));
        assert!(!html.contains("POST /kyc/start"), "project 10 filtered out");
        assert!(
            html.contains("Parent span not in view"),
            "11's parent is no longer drawn, so its row is marked"
        );
        assert!(
            html.contains("data-project=\"10\""),
            "legend keeps the toggle"
        );

        // A project outside the caller's scope matches nothing; it does not
        // widen the read and it does not appear.
        let foreign = body(
            org_page(&pool, member_of(&[20]), TRACE, Some("12"))
                .await
                .unwrap(),
        )
        .await;
        assert!(!foreign.contains("GET /far"));
        assert!(!foreign.contains("POST /kyc/start"));
    }

    #[tokio::test]
    async fn the_per_project_page_banners_only_when_others_contribute() {
        let pool = seeded().await;
        let html = body(project_page(&pool, member_of(&[20]), 10).await.unwrap()).await;
        assert!(html.contains("POST /kyc/start"));
        assert!(
            !html.contains("POST /users/registration"),
            "rendered rows stay project-local"
        );
        assert!(
            html.contains("1 more transaction") && html.contains("in 1 other project"),
            "the banner counts what the scope could read but the page does not show"
        );

        // A member of org 21 alone sees project 12's trace with nothing else on it.
        let alone = body(project_page(&pool, member_of(&[21]), 12).await.unwrap()).await;
        assert!(alone.contains("GET /far"));
        assert!(
            !alone.contains("more transaction"),
            "no banner without others"
        );
    }

    #[tokio::test]
    async fn the_per_project_page_404s_outside_the_callers_orgs() {
        let pool = seeded().await;
        assert!(
            project_page(&pool, member_of(&[20]), 12).await.is_err(),
            "a project in a foreign org is not found, not empty"
        );
    }
}
