use anyhow::Result;
use sqlx::Row;

use crate::db::sql;

use super::types::{
    Page, PagedResult, SpanSummary, TraceError, TraceSpan, TraceSummary, Waterfall, WaterfallRow,
};

pub async fn list_spans(
    pool: &crate::db::DbPool,
    project_id: u64,
    page: &Page,
) -> Result<PagedResult<SpanSummary>> {
    let count_row = sqlx::query(sql!("SELECT COUNT(*) FROM spans WHERE project_id = ?1"))
        .bind(project_id as i64)
        .fetch_one(pool)
        .await?;
    let total = count_row.get::<i64, _>(0);

    let rows = sqlx::query(sql!(
        "SELECT span_id, trace_id, timestamp, op, description, duration_ms
         FROM spans WHERE project_id = ?1
         ORDER BY timestamp DESC
         LIMIT ?2 OFFSET ?3"
    ))
    .bind(project_id as i64)
    .bind(page.limit as i64)
    .bind(page.offset as i64)
    .fetch_all(pool)
    .await?;

    let items = rows
        .iter()
        .map(|row| SpanSummary {
            span_id: row.get("span_id"),
            trace_id: row.get::<Option<String>, _>("trace_id").unwrap_or_default(),
            timestamp: row.get("timestamp"),
            op: row.get("op"),
            description: row.get("description"),
            duration_ms: row.get("duration_ms"),
        })
        .collect();

    Ok(PagedResult::from_page(items, total, page))
}

pub async fn get_trace_spans(pool: &crate::db::DbPool, trace_id: &str) -> Result<Vec<TraceSpan>> {
    let rows = sqlx::query(sql!(
        "SELECT span_id, parent_span_id, op, description, status, duration_ms, start_ms
         FROM spans WHERE trace_id = ?1
         ORDER BY timestamp
         LIMIT 10000"
    ))
    .bind(trace_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| TraceSpan {
            span_id: row.get("span_id"),
            parent_span_id: row.get("parent_span_id"),
            op: row.get("op"),
            description: row.get("description"),
            status: row.get("status"),
            duration_ms: row.get("duration_ms"),
            start_ms: row.get("start_ms"),
        })
        .collect())
}

/// Error events sharing this trace_id (LIMIT 50, newest first).
pub async fn get_trace_errors(
    pool: &crate::db::DbPool,
    project_id: u64,
    trace_id: &str,
) -> Result<Vec<TraceError>> {
    let rows = sqlx::query(sql!(
        "SELECT event_id, title, level, timestamp FROM events
         WHERE project_id = ?1 AND trace_id = ?2 AND item_type = 'event'
         ORDER BY timestamp DESC
         LIMIT 50"
    ))
    .bind(project_id as i64)
    .bind(trace_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| TraceError {
            event_id: row.get("event_id"),
            title: row.get("title"),
            level: row.get("level"),
            timestamp: row.get("timestamp"),
        })
        .collect())
}

/// The transaction event that owns this trace (name + duration), for the
/// waterfall root row. None when only standalone spans landed for the trace.
pub async fn get_trace_root(
    pool: &crate::db::DbPool,
    project_id: u64,
    trace_id: &str,
) -> Result<Option<crate::queries::types::TraceRoot>> {
    let row = sqlx::query(sql!(
        "SELECT transaction_name, duration_ms FROM events
         WHERE project_id = ?1 AND trace_id = ?2 AND item_type = 'transaction'
         ORDER BY timestamp DESC
         LIMIT 1"
    ))
    .bind(project_id as i64)
    .bind(trace_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| crate::queries::types::TraceRoot {
        name: r.get("transaction_name"),
        duration_ms: r.get("duration_ms"),
    }))
}

pub const MAX_WATERFALL_ROWS: usize = 2000;
const MAX_DEPTH: usize = 64;

/// Minimal projection the waterfall builder needs. Decoupled from `TraceSpan`
/// so the algorithm stays pure and trivially testable.
pub struct SpanRow {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub op: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub duration_ms: Option<i64>,
    pub start_ms: Option<i64>,
}

impl From<&TraceSpan> for SpanRow {
    fn from(s: &TraceSpan) -> Self {
        SpanRow {
            span_id: s.span_id.clone(),
            parent_span_id: s.parent_span_id.clone(),
            op: s.op.clone(),
            description: s.description.clone(),
            status: s.status.clone(),
            duration_ms: s.duration_ms,
            start_ms: s.start_ms,
        }
    }
}

/// Build a CSS waterfall from a flat span set. Pure: no DB, no allocation
/// beyond the result. Iterative DFS guards against cycles and pathological
/// depth so attacker/SDK-controlled parent pointers can't wedge the renderer.
pub fn build_waterfall(spans: &[SpanRow], root_duration_ms: i64) -> Waterfall {
    let span_count = spans.len();
    if spans.is_empty() {
        return Waterfall {
            total_ms: root_duration_ms.max(1),
            ..Default::default()
        };
    }

    let trace_start = spans.iter().filter_map(|s| s.start_ms).min().unwrap_or(0);
    let trace_end = spans
        .iter()
        .filter_map(|s| {
            s.start_ms
                .map(|st| st.saturating_add(s.duration_ms.unwrap_or(0)))
        })
        .max()
        .unwrap_or(trace_start);
    // Trace duration, shared with the traces list: wider of child-span extent and
    // the owning transaction's own duration.
    let span_extent_ms = (trace_end - trace_start).max(0);
    let total_ms = span_extent_ms.max(root_duration_ms).max(1);

    let present: std::collections::HashSet<&str> =
        spans.iter().map(|s| s.span_id.as_str()).collect();

    let mut children: std::collections::HashMap<&str, Vec<usize>> =
        std::collections::HashMap::new();
    let mut roots: Vec<usize> = Vec::new();
    for (i, s) in spans.iter().enumerate() {
        match &s.parent_span_id {
            Some(p) if present.contains(p.as_str()) => {
                children.entry(p.as_str()).or_default().push(i);
            }
            // Root, or orphan whose parent isn't in the set.
            _ => roots.push(i),
        }
    }

    let order_key = |i: usize| -> (i64, &str) {
        let s = &spans[i];
        (s.start_ms.unwrap_or(i64::MAX), s.span_id.as_str())
    };
    let sort_siblings = |v: &mut Vec<usize>| {
        v.sort_by(|&a, &b| order_key(a).cmp(&order_key(b)));
    };
    sort_siblings(&mut roots);
    for v in children.values_mut() {
        sort_siblings(v);
    }

    let mut visited: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut rows: Vec<WaterfallRow> = Vec::new();
    let mut truncated = false;

    // Stack of (span index, depth). Push roots in reverse so the first sibling
    // pops first (pre-order, siblings ascending by start).
    let mut stack: Vec<(usize, usize)> = roots.iter().rev().map(|&i| (i, 0)).collect();

    while let Some((idx, depth)) = stack.pop() {
        let s = &spans[idx];
        if visited.contains(s.span_id.as_str()) {
            continue;
        }
        if rows.len() >= MAX_WATERFALL_ROWS {
            truncated = true;
            break;
        }
        visited.insert(s.span_id.as_str());

        let offset_pct = match s.start_ms {
            Some(st) => ((st - trace_start) as f64 / total_ms as f64 * 100.0).clamp(0.0, 100.0),
            None => 0.0,
        };
        let mut width_pct = (s.duration_ms.unwrap_or(0) as f64 / total_ms as f64 * 100.0).max(0.5);
        if offset_pct + width_pct > 100.0 {
            width_pct = 100.0 - offset_pct;
        }

        rows.push(WaterfallRow {
            span_id: s.span_id.clone(),
            depth: depth.min(MAX_DEPTH),
            op: s.op.clone(),
            description: s.description.clone(),
            status: s.status.clone(),
            duration_ms: s.duration_ms,
            offset_pct,
            width_pct,
        });

        if let Some(kids) = children.get(s.span_id.as_str()) {
            let child_depth = (depth + 1).min(MAX_DEPTH);
            for &c in kids.iter().rev() {
                stack.push((c, child_depth));
            }
        }
    }

    Waterfall {
        rows,
        total_ms,
        span_count,
        truncated,
    }
}

pub async fn list_traces(
    pool: &crate::db::DbPool,
    project_id: u64,
    page: &Page,
) -> Result<PagedResult<TraceSummary>> {
    let count_row = sqlx::query(sql!(
        "SELECT COUNT(DISTINCT trace_id) FROM spans WHERE project_id = ?1 AND trace_id IS NOT NULL"
    ))
    .bind(project_id as i64)
    .fetch_one(pool)
    .await?;
    let total = count_row.get::<i64, _>(0);

    let rows = sqlx::query(sql!(
        "SELECT trace_id,
                COUNT(*) AS span_count,
                MIN(timestamp) AS first_timestamp,
                MAX(timestamp) AS last_timestamp,
                MAX(start_ms + COALESCE(duration_ms, 0)) - MIN(start_ms) AS span_extent_ms,
                (SELECT MAX(e.duration_ms) FROM events e
                 WHERE e.project_id = ?1 AND e.trace_id = spans.trace_id AND e.item_type = 'transaction') AS root_duration_ms,
                (SELECT MAX(e.transaction_name) FROM events e
                 WHERE e.project_id = ?1 AND e.trace_id = spans.trace_id AND e.item_type = 'transaction') AS root_txn_name
         FROM spans
         WHERE project_id = ?1 AND trace_id IS NOT NULL
         GROUP BY trace_id
         ORDER BY last_timestamp DESC
         LIMIT ?2 OFFSET ?3"
    ))
    .bind(project_id as i64)
    .bind(page.limit as i64)
    .bind(page.offset as i64)
    .fetch_all(pool)
    .await?;

    // transaction_name fallback for root_description when no stored root span supplies one.
    let mut fallback_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut items: Vec<TraceSummary> = Vec::with_capacity(rows.len());
    for row in &rows {
        let trace_id: String = row.get::<Option<String>, _>("trace_id").unwrap_or_default();
        let span_extent_ms = row
            .get::<Option<i64>, _>("span_extent_ms")
            .unwrap_or(0)
            .max(0);
        let root_duration_ms = row.get::<Option<i64>, _>("root_duration_ms").unwrap_or(0);
        if let Some(name) = row.get::<Option<String>, _>("root_txn_name") {
            fallback_names.insert(trace_id.clone(), name);
        }
        items.push(TraceSummary {
            trace_id,
            span_count: row.get::<i64, _>("span_count") as u64,
            first_timestamp: row.get("first_timestamp"),
            last_timestamp: row.get("last_timestamp"),
            root_op: None,
            root_description: None,
            total_duration_ms: Some(span_extent_ms.max(root_duration_ms)),
        });
    }

    if !items.is_empty() {
        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
            "SELECT trace_id, op, description FROM spans WHERE parent_span_id IS NULL AND trace_id IN ("
        );
        let mut sep = qb.separated(", ");
        for item in &items {
            sep.push_bind(item.trace_id.clone());
        }
        qb.push(") AND project_id = ");
        qb.push_bind(project_id as i64);
        let root_rows = qb.build().fetch_all(pool).await?;

        let mut root_map: std::collections::HashMap<String, (Option<String>, Option<String>)> =
            std::collections::HashMap::new();
        for row in &root_rows {
            let tid: String = row.get::<Option<String>, _>("trace_id").unwrap_or_default();
            root_map
                .entry(tid)
                .or_insert_with(|| (row.get("op"), row.get("description")));
        }

        for item in &mut items {
            if let Some((op, desc)) = root_map.remove(&item.trace_id) {
                item.root_op = op;
                item.root_description = desc;
            }
        }
    }

    // The transaction (root) is not stored in `spans`, so fall back to its name.
    for item in &mut items {
        if item.root_description.is_none() {
            if let Some(name) = fallback_names.remove(&item.trace_id) {
                item.root_description = Some(name);
            }
        }
    }

    // root_op lives only on the transaction event, not in `spans`; read it from
    // the transaction payload's contexts.trace.op.
    if items.iter().any(|i| i.root_op.is_none()) {
        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
            "SELECT trace_id, payload FROM events WHERE item_type = 'transaction' AND trace_id IN (",
        );
        let mut sep = qb.separated(", ");
        for item in &items {
            sep.push_bind(item.trace_id.clone());
        }
        qb.push(") AND project_id = ");
        qb.push_bind(project_id as i64);
        let txn_rows = qb.build().fetch_all(pool).await?;

        let mut op_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for row in &txn_rows {
            let tid: String = row.get::<Option<String>, _>("trace_id").unwrap_or_default();
            if op_map.contains_key(&tid) {
                continue;
            }
            let blob: Vec<u8> = row.get("payload");
            if let Some(op) = crate::queries::events::decompress_payload(&blob)
                .ok()
                .and_then(|p| {
                    p.get("contexts")
                        .and_then(|c| c.get("trace"))
                        .and_then(|t| t.get("op"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
            {
                op_map.insert(tid, op);
            }
        }

        for item in &mut items {
            if item.root_op.is_none() {
                item.root_op = op_map.remove(&item.trace_id);
            }
        }
    }

    Ok(PagedResult::from_page(items, total, page))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(id: &str, parent: Option<&str>, start: Option<i64>, dur: Option<i64>) -> SpanRow {
        SpanRow {
            span_id: id.to_string(),
            parent_span_id: parent.map(String::from),
            op: None,
            description: None,
            status: None,
            duration_ms: dur,
            start_ms: start,
        }
    }

    fn row<'a>(w: &'a Waterfall, id: &str) -> &'a WaterfallRow {
        w.rows
            .iter()
            .find(|r| r.span_id == id)
            .expect("row present")
    }

    fn waterfall_row(status: Option<&str>) -> WaterfallRow {
        WaterfallRow {
            span_id: "s".into(),
            depth: 0,
            op: None,
            description: None,
            status: status.map(String::from),
            duration_ms: Some(1),
            offset_pct: 0.0,
            width_pct: 1.0,
        }
    }

    #[test]
    fn is_error_classifies_status() {
        assert!(!waterfall_row(Some("ok")).is_error());
        assert!(!waterfall_row(Some("cancelled")).is_error());
        assert!(!waterfall_row(Some("unknown")).is_error());
        assert!(!waterfall_row(None).is_error());
        assert!(waterfall_row(Some("internal_error")).is_error());
        assert!(waterfall_row(Some("deadline_exceeded")).is_error());
        // bar color: ok green, error red, neutral gray
        assert_eq!(waterfall_row(Some("ok")).bar_color(), "#16a34a");
        assert_eq!(waterfall_row(Some("internal_error")).bar_color(), "#dc2626");
        assert_eq!(waterfall_row(None).bar_color(), "#9ca3af");
    }

    #[test]
    fn nesting_depth_and_sibling_order() {
        let spans = vec![
            span("root", None, Some(0), Some(100)),
            span("b", Some("root"), Some(50), Some(10)),
            span("a", Some("root"), Some(10), Some(10)),
        ];
        let w = build_waterfall(&spans, 0);
        assert_eq!(row(&w, "root").depth, 0);
        assert_eq!(row(&w, "a").depth, 1);
        assert_eq!(row(&w, "b").depth, 1);

        let order: Vec<&str> = w.rows.iter().map(|r| r.span_id.as_str()).collect();
        assert_eq!(order, vec!["root", "a", "b"]);
    }

    #[test]
    fn orphans_attach_at_depth_zero() {
        let spans = vec![
            span("c1", Some("ghost"), Some(0), Some(10)),
            span("c2", Some("ghost"), Some(5), Some(10)),
        ];
        let w = build_waterfall(&spans, 0);
        assert_eq!(row(&w, "c1").depth, 0);
        assert_eq!(row(&w, "c2").depth, 0);
        assert_eq!(w.rows.len(), 2);
    }

    #[test]
    fn cycle_terminates_each_span_once() {
        let spans = vec![
            span("A", Some("B"), Some(0), Some(10)),
            span("B", Some("A"), Some(0), Some(10)),
        ];
        let w = build_waterfall(&spans, 0);
        assert_eq!(w.span_count, 2);
        // No root in a pure 2-cycle -> no rows, but must terminate.
        let mut ids: Vec<&str> = w.rows.iter().map(|r| r.span_id.as_str()).collect();
        let len = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), len);
    }

    #[test]
    fn chain_with_root_visits_each_once() {
        let spans = vec![
            span("root", None, Some(0), Some(100)),
            span("A", Some("root"), Some(0), Some(50)),
            span("B", Some("A"), Some(0), Some(50)),
            span("C", Some("B"), Some(0), Some(10)),
        ];
        let w = build_waterfall(&spans, 0);
        let count = w.rows.len();
        let mut ids: Vec<&str> = w.rows.iter().map(|r| r.span_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "no span emitted twice");
        assert_eq!(count, 4);
        assert_eq!(row(&w, "C").depth, 3);
    }

    #[test]
    fn deep_chain_does_not_panic_and_clamps_depth() {
        let mut spans = vec![span("s0", None, Some(0), Some(1))];
        for i in 1..100 {
            let parent = format!("s{}", i - 1);
            spans.push(span(&format!("s{i}"), Some(&parent), Some(0), Some(1)));
        }
        let w = build_waterfall(&spans, 0);
        assert_eq!(w.rows.len(), 100);
        let max_depth = w.rows.iter().map(|r| r.depth).max().unwrap();
        assert_eq!(max_depth, MAX_DEPTH);
    }

    #[test]
    fn null_geometry_offset_zero_and_no_div_by_zero() {
        let spans = vec![
            span("a", None, None, Some(10)),
            span("b", None, Some(0), Some(20)),
        ];
        let w = build_waterfall(&spans, 0);
        assert_eq!(row(&w, "a").offset_pct, 0.0);
        assert!(w.total_ms >= 1);
    }

    #[test]
    fn all_none_starts_total_ms_one() {
        let spans = vec![span("a", None, None, None), span("b", None, None, None)];
        let w = build_waterfall(&spans, 0);
        assert_eq!(w.total_ms, 1);
        assert_eq!(w.rows.len(), 2);
    }

    #[test]
    fn empty_set_total_ms_one() {
        let w = build_waterfall(&[], 0);
        assert_eq!(w.total_ms, 1);
        assert_eq!(w.span_count, 0);
        assert!(w.rows.is_empty());
    }

    #[test]
    fn adversarial_start_ms_does_not_overflow() {
        // Hostile SDK timestamps near i64::MAX must not panic (debug) or wrap to a
        // negative trace_end (release).
        let spans = vec![span("a", None, Some(i64::MAX - 1), Some(1000))];
        let w = build_waterfall(&spans, 0);
        assert!(w.total_ms >= 1);
        assert_eq!(w.rows.len(), 1);
    }

    #[test]
    fn zero_duration_min_width() {
        let spans = vec![span("a", None, Some(0), Some(0))];
        let w = build_waterfall(&spans, 0);
        assert_eq!(row(&w, "a").width_pct, 0.5);
    }

    #[test]
    fn offset_plus_width_clamped() {
        let spans = vec![
            span("a", None, Some(0), Some(100)),
            span("b", None, Some(100), Some(1000)),
        ];
        let w = build_waterfall(&spans, 0);
        for r in &w.rows {
            assert!(
                r.offset_pct + r.width_pct <= 100.0 + f64::EPSILON,
                "row {} over 100",
                r.span_id
            );
        }
    }

    #[test]
    fn truncation_caps_rows() {
        let mut spans = Vec::new();
        for i in 0..(MAX_WATERFALL_ROWS + 50) {
            spans.push(span(&format!("s{i}"), None, Some(i as i64), Some(1)));
        }
        let w = build_waterfall(&spans, 0);
        assert_eq!(w.rows.len(), MAX_WATERFALL_ROWS);
        assert!(w.truncated);
        assert_eq!(w.span_count, MAX_WATERFALL_ROWS + 50);
    }

    #[test]
    fn root_duration_widens_total_and_scales_children() {
        // Child extent is 50ms but the owning transaction lasts 1000ms; the axis
        // must follow the transaction, and the 50ms child render at ~5%.
        let spans = vec![span("a", None, Some(0), Some(50))];
        let w = build_waterfall(&spans, 1000);
        assert_eq!(w.total_ms, 1000);
        assert!((row(&w, "a").width_pct - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn span_extent_wins_when_larger_than_root() {
        let spans = vec![span("a", None, Some(0), Some(2000))];
        let w = build_waterfall(&spans, 500);
        assert_eq!(w.total_ms, 2000);
    }

    #[test]
    fn not_truncated_under_cap() {
        let spans = vec![span("a", None, Some(0), Some(10))];
        let w = build_waterfall(&spans, 0);
        assert!(!w.truncated);
    }

    async fn insert_evt(
        pool: &crate::db::DbPool,
        event_id: &str,
        item_type: &str,
        project_id: i64,
        trace_id: Option<&str>,
        timestamp: i64,
    ) {
        let compressed = zstd::encode_all([0u8; 0].as_slice(), 3).unwrap();
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, title, level, trace_id)
             VALUES (?1, ?2, ?3, ?4, 'testkey', ?5, ?6, 'error', ?7)"
        ))
        .bind(event_id)
        .bind(item_type)
        .bind(&compressed)
        .bind(project_id)
        .bind(timestamp)
        .bind(event_id)
        .bind(trace_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn trace_errors_only_event_rows_for_trace() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        // matching error event
        insert_evt(&pool, "e1", "event", 1, Some("t1"), 100).await;
        // a transaction sharing the trace -- must be excluded
        insert_evt(&pool, "tx1", "transaction", 1, Some("t1"), 110).await;
        // an error on a different trace -- must be excluded
        insert_evt(&pool, "e2", "event", 1, Some("t2"), 120).await;
        // an error on a different project -- must be excluded
        insert_evt(&pool, "e3", "event", 2, Some("t1"), 130).await;
        // second matching error, newer
        insert_evt(&pool, "e4", "event", 1, Some("t1"), 200).await;

        let errors = get_trace_errors(&pool, 1, "t1").await.unwrap();
        let ids: Vec<&str> = errors.iter().map(|e| e.event_id.as_str()).collect();
        assert_eq!(ids, vec!["e4", "e1"]); // newest first, only event/trace/project match
    }
}
