//! The org-wide traces list: one row per trace, across every project the caller
//! can read.
//!
//! Distinct from `spans::list_traces`, which is per project and reads `spans`.
//! This reads transaction rows in `events`, so a project that contributed a
//! transaction and no child spans still shows up — which is the whole point of
//! a cross-project list.

use anyhow::Result;
use sqlx::Row;

use crate::db::DbRowExt;

use super::spans::{since_bound, TraceScope, SPAN_AGG_SCAN_LIMIT};
use super::types::{OrgTraceSummary, Page, PagedResult, TraceProjectCount, TraceRollup};

/// What the caller asked to see, on top of their entitlement. Never widens the
/// read: `TraceScope` is the only `WHERE`-level project predicate.
#[derive(Debug, Default, Clone)]
pub struct TraceListFilter {
    /// `?projects=` — a membership test, not a narrowing. A trace is kept when
    /// *any* of its projects is named.
    pub projects: Option<Vec<i64>>,
    /// Keep only traces the scan saw in more than one project.
    pub multi_project_only: bool,
}

impl TraceListFilter {
    fn project_ids(&self) -> Option<&[i64]> {
        self.projects.as_deref().filter(|ids| !ids.is_empty())
    }
}

/// The bounded scan both the count and the row query group over. Emitted from
/// one place so the two can never drift — that identity is what keeps
/// pagination consistent with what the listing can actually return.
fn push_recent_transactions(
    qb: &mut sqlx::QueryBuilder<crate::db::Db>,
    scope: &TraceScope,
    since: i64,
    scan_limit: i64,
) {
    qb.push(
        "(SELECT trace_id, project_id, timestamp, start_ms, duration_ms, parent_span_id, transaction_name
          FROM events
          WHERE item_type = 'transaction' AND trace_id IS NOT NULL AND timestamp >= ",
    );
    qb.push_bind(since);
    scope.push_predicate(qb, "project_id");
    qb.push(" ORDER BY timestamp DESC LIMIT ");
    qb.push_bind(scan_limit);
    // Postgres requires an alias on a derived table; SQLite tolerates one.
    qb.push(") recent");
}

/// The caller's filters, as `HAVING` over the grouped scan. `?projects=` is a
/// membership test rather than a `WHERE` narrowing on purpose: narrowing the
/// scan would compute `COUNT(DISTINCT project_id)` over the narrowed set, so
/// "project A" plus "spans more than one project" would return nothing —
/// precisely the flow this page exists for.
fn push_having(qb: &mut sqlx::QueryBuilder<crate::db::Db>, filter: &TraceListFilter) {
    let mut emitted = false;
    if filter.multi_project_only {
        qb.push(" HAVING COUNT(DISTINCT project_id) > 1");
        emitted = true;
    }
    if let Some(ids) = filter.project_ids() {
        qb.push(if emitted { " AND " } else { " HAVING " });
        qb.push("MAX(CASE WHEN project_id IN (");
        {
            let mut sep = qb.separated(", ");
            for id in ids {
                sep.push_bind(*id);
            }
        }
        qb.push(") THEN 1 ELSE 0 END) = 1");
    }
}

/// Traces across the caller's scope, newest last-seen first.
///
/// Four limits are accepted rather than worked around:
///
/// - A trace whose second project contributed a transaction older than the scan
///   window reads as single-project to `COUNT(DISTINCT project_id)`.
/// - `MIN` over TEXT picks the root name by collation order, so a trace with two
///   parentless transactions has no defined winner across backends.
/// - The group-by is scan-bounded and [`trace_event_rollups`] is not, so the two
///   can disagree about a trace's project set at the scan boundary. The jobs are
///   divided rather than reconciled: **the rollup is authoritative for
///   everything displayed** (the chips and the project count in the row), and
///   `project_count` here drives *only* the multi-project toggle and the
///   `?projects=` membership test. The residue is one explainable case — a trace
///   can carry two chips and still be dropped by the toggle, because one of its
///   projects fell outside the scan window.
/// - For the same reason `transaction_count` is scan-truncated where the chip
///   counts are not, so the chips can sum to more than the row's total.
///
/// `scan_limit` bounds rows *returned* by the inner scan, not rows examined: the
/// scope predicate filters as the index is walked, so an org holding a small
/// share of an install's transactions walks further to fill it.
pub async fn list_org_traces(
    pool: &crate::db::DbPool,
    scope: &TraceScope,
    filter: &TraceListFilter,
    page: &Page,
    since_ts: Option<i64>,
) -> Result<PagedResult<OrgTraceSummary>> {
    list_org_traces_with_scan_limit(pool, scope, filter, page, since_ts, SPAN_AGG_SCAN_LIMIT).await
}

pub(crate) async fn list_org_traces_with_scan_limit(
    pool: &crate::db::DbPool,
    scope: &TraceScope,
    filter: &TraceListFilter,
    page: &Page,
    since_ts: Option<i64>,
    scan_limit: i64,
) -> Result<PagedResult<OrgTraceSummary>> {
    if scope.is_empty() {
        return Ok(PagedResult::from_page(Vec::new(), 0, page));
    }
    let since = since_bound(since_ts);

    // `COUNT(DISTINCT trace_id)` cannot carry a HAVING, so the grouped scan is
    // nested and counted as rows.
    let mut qb =
        sqlx::QueryBuilder::<crate::db::Db>::new("SELECT COUNT(*) FROM (SELECT trace_id FROM ");
    push_recent_transactions(&mut qb, scope, since, scan_limit);
    qb.push(" GROUP BY trace_id");
    push_having(&mut qb, filter);
    qb.push(") g");
    let total = qb.build().fetch_one(pool).await?.get::<i64, _>(0);

    let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
        "SELECT trace_id,
                COUNT(*) AS transaction_count,
                COUNT(DISTINCT project_id) AS project_count,
                MIN(timestamp) AS first_timestamp,
                MAX(timestamp) AS last_timestamp,
                MAX(start_ms + COALESCE(duration_ms, 0)) - MIN(start_ms) AS extent_ms,
                MIN(CASE WHEN parent_span_id IS NULL THEN transaction_name END) AS root_name,
                MIN(transaction_name) AS any_name
         FROM ",
    );
    push_recent_transactions(&mut qb, scope, since, scan_limit);
    qb.push(" GROUP BY trace_id");
    push_having(&mut qb, filter);
    // `trace_id` is the group key, so it breaks ties totally. Without it,
    // traces sharing a second — routine on an org-wide list, where timestamps
    // are Unix seconds — can repeat across pages or be skipped entirely.
    qb.push(" ORDER BY last_timestamp DESC, trace_id DESC LIMIT ");
    qb.push_bind(page.limit as i64);
    qb.push(" OFFSET ");
    qb.push_bind(page.offset as i64);

    let rows = qb.build().fetch_all(pool).await?;
    let items = rows
        .iter()
        .map(|row| OrgTraceSummary {
            trace_id: row.get_opt_string("trace_id").unwrap_or_default(),
            transaction_count: row.get_u64("transaction_count"),
            project_count: row.get_u64("project_count"),
            first_timestamp: row.get("first_timestamp"),
            last_timestamp: row.get("last_timestamp"),
            extent_ms: row.get::<Option<i64>, _>("extent_ms").map(|ms| ms.max(0)),
            root_name: row
                .get_opt_string("root_name")
                .or_else(|| row.get_opt_string("any_name")),
        })
        .collect();

    Ok(PagedResult::from_page(items, total, page))
}

/// Per-(trace, project) transaction counts and per-trace error counts, in one
/// read over `events`. Keyed on the visible page's trace ids so it rides
/// `idx_events_trace`.
///
/// Project chips come from the transaction rows only, so the chip set agrees
/// with what the multi-project toggle filters on.
pub async fn trace_event_rollups(
    pool: &crate::db::DbPool,
    trace_ids: &[String],
    scope: &TraceScope,
    since_ts: Option<i64>,
) -> Result<std::collections::HashMap<String, TraceRollup>> {
    let mut out: std::collections::HashMap<String, TraceRollup> = std::collections::HashMap::new();
    if trace_ids.is_empty() || scope.is_empty() {
        return Ok(out);
    }

    let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
        "SELECT trace_id, project_id, item_type, COUNT(*) AS row_count
         FROM events
         WHERE item_type IN ('transaction', 'event') AND timestamp >= ",
    );
    qb.push_bind(since_bound(since_ts));
    qb.push(" AND trace_id IN (");
    {
        let mut sep = qb.separated(", ");
        for id in trace_ids {
            sep.push_bind(id.clone());
        }
    }
    qb.push(")");
    scope.push_predicate(&mut qb, "project_id");
    qb.push(" GROUP BY trace_id, project_id, item_type");

    let rows = qb.build().fetch_all(pool).await?;
    for row in &rows {
        let trace_id: String = row.get_opt_string("trace_id").unwrap_or_default();
        let count = row.get_u64("row_count");
        let entry = out.entry(trace_id).or_default();
        match row.get_opt_string("item_type").as_deref() {
            Some("transaction") => entry.projects.push(TraceProjectCount {
                project_id: row.get("project_id"),
                transaction_count: count,
            }),
            _ => entry.error_count += count,
        }
    }
    for rollup in out.values_mut() {
        rollup.projects.sort_by(|a, b| {
            b.transaction_count
                .cmp(&a.transaction_count)
                .then(a.project_id.cmp(&b.project_id))
        });
    }

    Ok(out)
}

/// Span counts for the visible page's traces, within the caller's scope. Rides
/// `idx_spans_trace`; the trace ids are the driver, so there is no time bound —
/// which is why a row's span count can cover a wider window than its error
/// count, and why the two are read separately rather than joined.
pub async fn trace_span_counts(
    pool: &crate::db::DbPool,
    trace_ids: &[String],
    scope: &TraceScope,
) -> Result<std::collections::HashMap<String, u64>> {
    if trace_ids.is_empty() || scope.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
        "SELECT trace_id, COUNT(*) AS row_count FROM spans WHERE trace_id IN (",
    );
    {
        let mut sep = qb.separated(", ");
        for id in trace_ids {
            sep.push_bind(id.clone());
        }
    }
    qb.push(")");
    scope.push_predicate(&mut qb, "project_id");
    qb.push(" GROUP BY trace_id");

    let rows = qb.build().fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|row| {
            (
                row.get_opt_string("trace_id").unwrap_or_default(),
                row.get_u64("row_count"),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{sql, DbPool};

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccc";

    fn page() -> Page {
        Page::new(None, None)
    }

    /// Projects 10, 11 in org 20; project 12 in org 21.
    /// Trace A spans 10 and 11, trace B is project 10 only, trace C is 11 and 12
    /// (so it crosses orgs).
    async fn seeded() -> DbPool {
        let pool = crate::db::open_test_pool().await;
        for (org_id, slug) in [(20i64, "alpha"), (21, "beta")] {
            sqlx::query(sql!(
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
            sqlx::query(sql!(
                "INSERT INTO projects (project_id, org_id, name) VALUES (?1, ?2, ?3)"
            ))
            .bind(project_id)
            .bind(org_id)
            .bind(name)
            .execute(&pool)
            .await
            .unwrap();
        }

        txn(
            &pool,
            "e1",
            10,
            A,
            "POST /kyc/start",
            1_700_000_000,
            None,
            Some(1_700_000_000_000),
        )
        .await;
        txn(
            &pool,
            "e2",
            11,
            A,
            "POST /registration",
            1_700_000_001,
            Some("a000"),
            Some(1_700_000_000_050),
        )
        .await;
        txn(
            &pool,
            "e3",
            10,
            B,
            "GET /health",
            1_700_000_002,
            None,
            Some(1_700_000_002_000),
        )
        .await;
        txn(
            &pool,
            "e4",
            11,
            C,
            "GET /jwks",
            1_700_000_003,
            None,
            Some(1_700_000_003_000),
        )
        .await;
        txn(
            &pool,
            "e5",
            12,
            C,
            "GET /far",
            1_700_000_004,
            Some("c000"),
            Some(1_700_000_003_010),
        )
        .await;
        pool
    }

    #[allow(clippy::too_many_arguments)]
    async fn txn(
        pool: &DbPool,
        event_id: &str,
        project_id: i64,
        trace_id: &str,
        name: &str,
        timestamp: i64,
        parent: Option<&str>,
        start_ms: Option<i64>,
    ) {
        let blob = zstd::encode_all([0u8; 0].as_slice(), 3).unwrap();
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp,
                                 transaction_name, trace_id, duration_ms, parent_span_id, start_ms)
             VALUES (?1, 'transaction', ?2, ?3, 'k', ?4, ?5, ?6, 100, ?7, ?8)"
        ))
        .bind(event_id)
        .bind(&blob)
        .bind(project_id)
        .bind(timestamp)
        .bind(name)
        .bind(trace_id)
        .bind(parent)
        .bind(start_ms)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn error(pool: &DbPool, event_id: &str, project_id: i64, trace_id: &str) {
        let blob = zstd::encode_all([0u8; 0].as_slice(), 3).unwrap();
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp,
                                 title, level, trace_id)
             VALUES (?1, 'event', ?2, ?3, 'k', 1700000005, 'boom', 'error', ?4)"
        ))
        .bind(event_id)
        .bind(&blob)
        .bind(project_id)
        .bind(trace_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn ids(pool: &DbPool, scope: &TraceScope, filter: &TraceListFilter) -> Vec<String> {
        list_org_traces(pool, scope, filter, &page(), None)
            .await
            .unwrap()
            .items
            .into_iter()
            .map(|t| t.trace_id)
            .collect()
    }

    #[tokio::test]
    async fn it_groups_by_trace_and_counts_distinct_projects() {
        let pool = seeded().await;
        let result = list_org_traces(
            &pool,
            &TraceScope::All,
            &TraceListFilter::default(),
            &page(),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.total, 3);
        // Newest last-seen first: C (…004), B (…002), A (…001).
        let order: Vec<&str> = result.items.iter().map(|t| t.trace_id.as_str()).collect();
        assert_eq!(order, vec![C, B, A]);

        let a = result.items.iter().find(|t| t.trace_id == A).unwrap();
        assert_eq!(a.transaction_count, 2);
        assert_eq!(a.project_count, 2);
        assert_eq!(a.first_timestamp, 1_700_000_000);
        assert_eq!(a.last_timestamp, 1_700_000_001);
        // 1_700_000_000_050 + 100 - 1_700_000_000_000
        assert_eq!(a.extent_ms, Some(150));
        assert_eq!(a.root_name.as_deref(), Some("POST /kyc/start"));

        let b = result.items.iter().find(|t| t.trace_id == B).unwrap();
        assert_eq!(b.project_count, 1);
    }

    #[tokio::test]
    async fn the_multi_project_filter_excludes_single_project_traces() {
        let pool = seeded().await;
        let filter = TraceListFilter {
            multi_project_only: true,
            ..Default::default()
        };
        let mut got = ids(&pool, &TraceScope::All, &filter).await;
        got.sort();
        assert_eq!(got, vec![A.to_string(), C.to_string()]);

        let result = list_org_traces(&pool, &TraceScope::All, &filter, &page(), None)
            .await
            .unwrap();
        assert_eq!(result.total, 2, "the count carries the same HAVING");
    }

    #[tokio::test]
    async fn the_projects_filter_admits_through_any_id_and_composes_with_multi() {
        let pool = seeded().await;
        // Project 10 alone: traces A and B, both of which it contributed to.
        let mut got = ids(
            &pool,
            &TraceScope::All,
            &TraceListFilter {
                projects: Some(vec![10]),
                multi_project_only: false,
            },
        )
        .await;
        got.sort();
        assert_eq!(got, vec![A.to_string(), B.to_string()]);

        // The membership test never narrows the group: A still counts two
        // projects, so it survives the multi toggle.
        assert_eq!(
            ids(
                &pool,
                &TraceScope::All,
                &TraceListFilter {
                    projects: Some(vec![10]),
                    multi_project_only: true,
                },
            )
            .await,
            vec![A.to_string()],
        );

        // Any one of several ids admits the trace.
        let mut both = ids(
            &pool,
            &TraceScope::All,
            &TraceListFilter {
                projects: Some(vec![10, 12]),
                multi_project_only: false,
            },
        )
        .await;
        both.sort();
        assert_eq!(both, vec![A.to_string(), B.to_string(), C.to_string()]);

        // An id nothing contributed under matches no trace, rather than widening.
        assert!(ids(
            &pool,
            &TraceScope::All,
            &TraceListFilter {
                projects: Some(vec![999]),
                multi_project_only: false,
            },
        )
        .await
        .is_empty());
    }

    #[tokio::test]
    async fn the_scan_limit_bounds_to_the_newest_transactions() {
        let pool = seeded().await;
        // Two newest transaction rows are e5 and e4, both on trace C.
        let result = list_org_traces_with_scan_limit(
            &pool,
            &TraceScope::All,
            &TraceListFilter::default(),
            &page(),
            None,
            2,
        )
        .await
        .unwrap();
        assert_eq!(
            result
                .items
                .iter()
                .map(|t| t.trace_id.as_str())
                .collect::<Vec<_>>(),
            vec![C]
        );
        assert_eq!(result.total, 1, "the count runs over the same bounded scan");
    }

    #[tokio::test]
    async fn the_scope_is_the_only_project_predicate() {
        let pool = seeded().await;
        assert!(
            ids(
                &pool,
                &TraceScope::Orgs(Vec::new()),
                &TraceListFilter::default()
            )
            .await
            .is_empty(),
            "an empty entitlement selects nothing"
        );
        assert_eq!(
            ids(&pool, &TraceScope::All, &TraceListFilter::default())
                .await
                .len(),
            3,
            "a superuser sees every trace"
        );

        // Org 21 owns only project 12, which contributed to trace C alone — and
        // C then reads as single-project, because the other hop is unreadable.
        let result = list_org_traces(
            &pool,
            &TraceScope::Orgs(vec![21]),
            &TraceListFilter::default(),
            &page(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].trace_id, C);
        assert_eq!(result.items[0].project_count, 1);

        // `?projects=` may only subtract. A foreign id under a narrow scope
        // matches nothing rather than reaching the project it names.
        assert!(
            ids(
                &pool,
                &TraceScope::Orgs(vec![21]),
                &TraceListFilter {
                    projects: Some(vec![10]),
                    multi_project_only: false,
                },
            )
            .await
            .is_empty(),
            "org 21 must not reach org 20's project through the query string"
        );
        assert!(ids(
            &pool,
            &TraceScope::Orgs(vec![20]),
            &TraceListFilter {
                projects: Some(vec![12]),
                multi_project_only: false,
            },
        )
        .await
        .is_empty());

        // The one case that separates a HAVING membership test from a WHERE
        // narrowing: narrowing the scan to project 12 would leave trace C
        // counting one project, and the multi toggle would then drop it.
        assert_eq!(
            ids(
                &pool,
                &TraceScope::Orgs(vec![20, 21]),
                &TraceListFilter {
                    projects: Some(vec![12]),
                    multi_project_only: true,
                },
            )
            .await,
            vec![C.to_string()],
        );

        // `Projects` entitles exactly its list, through the same path.
        assert_eq!(
            ids(
                &pool,
                &TraceScope::Projects(vec![12]),
                &TraceListFilter::default()
            )
            .await,
            vec![C.to_string()],
        );
    }

    #[tokio::test]
    async fn the_period_bound_drops_older_traces() {
        let pool = seeded().await;
        // Everything seeded sits at 1_700_000_000..=1_700_000_004.
        assert_eq!(
            ids(&pool, &TraceScope::All, &TraceListFilter::default())
                .await
                .len(),
            3
        );
        let recent = list_org_traces(
            &pool,
            &TraceScope::All,
            &TraceListFilter::default(),
            &page(),
            Some(1_700_000_003),
        )
        .await
        .unwrap();
        assert_eq!(
            recent
                .items
                .iter()
                .map(|t| t.trace_id.as_str())
                .collect::<Vec<_>>(),
            vec![C],
            "only the hops inside the window are grouped"
        );
        assert_eq!(recent.total, 1);
    }

    #[tokio::test]
    async fn a_trace_across_two_of_the_callers_orgs_appears_once() {
        let pool = seeded().await;
        let result = list_org_traces(
            &pool,
            &TraceScope::Orgs(vec![20, 21]),
            &TraceListFilter {
                multi_project_only: true,
                ..Default::default()
            },
            &page(),
            None,
        )
        .await
        .unwrap();
        let c = result.items.iter().filter(|t| t.trace_id == C).count();
        assert_eq!(c, 1, "one row, not one per org");
        let c = result.items.iter().find(|t| t.trace_id == C).unwrap();
        assert_eq!(c.project_count, 2);
    }

    #[tokio::test]
    async fn extent_ms_is_none_when_no_row_carries_a_start() {
        let pool = seeded().await;
        let legacy = "dddddddddddddddddddddddddddddddd";
        txn(
            &pool,
            "e6",
            10,
            legacy,
            "POST /old",
            1_700_000_006,
            None,
            None,
        )
        .await;
        let result = list_org_traces(
            &pool,
            &TraceScope::All,
            &TraceListFilter::default(),
            &page(),
            None,
        )
        .await
        .unwrap();
        let row = result.items.iter().find(|t| t.trace_id == legacy).unwrap();
        assert_eq!(row.extent_ms, None);
    }

    #[tokio::test]
    async fn rollups_carry_the_chips_and_the_error_count_at_scope() {
        let pool = seeded().await;
        error(&pool, "x1", 10, A).await;
        error(&pool, "x2", 11, A).await;

        let all = trace_event_rollups(
            &pool,
            &[A.to_string(), C.to_string()],
            &TraceScope::All,
            None,
        )
        .await
        .unwrap();
        let a = all.get(A).unwrap();
        assert_eq!(
            a.projects,
            vec![
                TraceProjectCount {
                    project_id: 10,
                    transaction_count: 1
                },
                TraceProjectCount {
                    project_id: 11,
                    transaction_count: 1
                },
            ]
        );
        assert_eq!(a.error_count, 2);
        assert_eq!(all.get(C).unwrap().error_count, 0);

        // Scoped to org 21, only project 12's contribution to C is visible.
        let scoped = trace_event_rollups(
            &pool,
            &[A.to_string(), C.to_string()],
            &TraceScope::Orgs(vec![21]),
            None,
        )
        .await
        .unwrap();
        assert!(scoped.get(A).is_none(), "org 21 reads nothing on trace A");
        assert_eq!(
            scoped.get(C).unwrap().projects,
            vec![TraceProjectCount {
                project_id: 12,
                transaction_count: 1
            }]
        );
    }

    #[tokio::test]
    async fn span_counts_are_scoped_to_the_caller() {
        let pool = seeded().await;
        let blob = zstd::encode_all([0u8; 0].as_slice(), 3).unwrap();
        for (span_id, project_id) in [("s1", 10i64), ("s2", 11)] {
            sqlx::query(sql!(
                "INSERT INTO spans (span_id, payload, project_id, public_key, timestamp, trace_id, op)
                 VALUES (?1, ?2, ?3, 'k', 1700000000, ?4, 'db')"
            ))
            .bind(span_id)
            .bind(&blob)
            .bind(project_id)
            .bind(A)
            .execute(&pool)
            .await
            .unwrap();
        }

        let all = trace_span_counts(&pool, &[A.to_string()], &TraceScope::All)
            .await
            .unwrap();
        assert_eq!(all.get(A), Some(&2));

        let narrowed = trace_span_counts(&pool, &[A.to_string()], &TraceScope::Projects(vec![10]))
            .await
            .unwrap();
        assert_eq!(narrowed.get(A), Some(&1));

        assert!(
            trace_span_counts(&pool, &[A.to_string()], &TraceScope::Orgs(Vec::new()))
                .await
                .unwrap()
                .is_empty()
        );
    }
}
