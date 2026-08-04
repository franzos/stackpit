use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::db::DbRowExt;

use super::types::{
    Page, PagedResult, SpanAggRow, SpanAggregation, SpanSummary, TraceError, TraceSpan,
    TraceSummary, Waterfall, WaterfallGap, WaterfallRow,
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
            trace_id: row.get_opt_string("trace_id").unwrap_or_default(),
            timestamp: row.get("timestamp"),
            op: row.get("op"),
            description: row.get("description"),
            duration_ms: row.get("duration_ms"),
        })
        .collect();

    Ok(PagedResult::from_page(items, total, page))
}

/// Newest spans to scan when aggregating (by (op, description) or by trace).
/// Bounds the read so a busy project can't force an unbounded table scan; the
/// spans page has no period filter, so we cap on recency via the
/// (project_id, timestamp) index.
const SPAN_AGG_SCAN_LIMIT: i64 = 50_000;
/// Most (op, description) groups to render; the tail is dropped and flagged.
pub const MAX_SPAN_GROUPS: usize = 100;

/// Exact nearest-rank percentile over a *sorted-ascending* slice. `p` is a
/// fraction in 0.0..=1.0. Returns 0 for an empty slice.
fn percentile_exact(sorted: &[i64], p: f64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let n = sorted.len();
    let rank = (p * n as f64).ceil() as usize;
    sorted[rank.clamp(1, n) - 1]
}

/// Fold one group's raw durations into count/p50/p95/avg. Consumes and sorts
/// `durations` in place. Pure, so the percentile logic is unit-testable.
fn build_span_agg(
    op: Option<String>,
    description: Option<String>,
    mut durations: Vec<i64>,
) -> SpanAggRow {
    durations.sort_unstable();
    let count = durations.len() as u64;
    // i128 sum so adversarial SDK durations can't overflow before the divide.
    let sum: i128 = durations.iter().map(|&d| d as i128).sum();
    let avg_ms = if count > 0 {
        (sum / count as i128) as i64
    } else {
        0
    };
    SpanAggRow {
        op,
        description,
        count,
        p50_ms: percentile_exact(&durations, 0.50),
        p95_ms: percentile_exact(&durations, 0.95),
        avg_ms,
    }
}

/// Aggregate the project's recent spans by (op, description), computing exact
/// percentiles in Rust from raw `duration_ms`. Rows with NULL durations are
/// skipped. Sorted by count desc (p95 desc tiebreak) and capped to
/// `MAX_SPAN_GROUPS`.
pub async fn aggregate_spans(pool: &crate::db::DbPool, project_id: u64) -> Result<SpanAggregation> {
    let rows = sqlx::query(sql!(
        "SELECT op, description, duration_ms
         FROM spans
         WHERE project_id = ?1 AND duration_ms IS NOT NULL
         ORDER BY timestamp DESC
         LIMIT ?2"
    ))
    .bind(project_id as i64)
    .bind(SPAN_AGG_SCAN_LIMIT)
    .fetch_all(pool)
    .await?;

    let mut by_group: std::collections::HashMap<(Option<String>, Option<String>), Vec<i64>> =
        std::collections::HashMap::new();
    for row in &rows {
        let op: Option<String> = row.get("op");
        let description: Option<String> = row.get("description");
        by_group
            .entry((op, description))
            .or_default()
            .push(row.get("duration_ms"));
    }

    let mut groups: Vec<SpanAggRow> = by_group
        .into_iter()
        .map(|((op, description), durations)| build_span_agg(op, description, durations))
        .collect();

    groups.sort_by(|a, b| b.count.cmp(&a.count).then(b.p95_ms.cmp(&a.p95_ms)));
    let truncated = groups.len() > MAX_SPAN_GROUPS;
    groups.truncate(MAX_SPAN_GROUPS);

    Ok(SpanAggregation { groups, truncated })
}

/// A trace id is shared across projects in a distributed trace, so every read
/// is project-scoped: a caller entitled to one project must not reach another's
/// spans by presenting its id.
pub async fn get_trace_spans_for_project(
    pool: &crate::db::DbPool,
    project_id: u64,
    trace_id: &str,
) -> Result<Vec<TraceSpan>> {
    let rows = sqlx::query(sql!(
        "SELECT span_id, parent_span_id, op, description, status, duration_ms, start_ms
         FROM spans WHERE project_id = ?1 AND trace_id = ?2
         ORDER BY timestamp
         LIMIT 10000"
    ))
    .bind(project_id as i64)
    .bind(trace_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(map_trace_span).collect())
}

fn map_trace_span(row: &crate::db::DbRow) -> TraceSpan {
    TraceSpan {
        span_id: row.get("span_id"),
        parent_span_id: row.get("parent_span_id"),
        op: row.get("op"),
        description: row.get("description"),
        status: row.get("status"),
        duration_ms: row.get("duration_ms"),
        start_ms: row.get("start_ms"),
    }
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

/// Piecewise real-ms -> display-percent map that collapses long idle gaps.
///
/// A single span that starts far after the rest (e.g. a click 40s after page
/// load) would otherwise stretch the axis so every earlier span renders as a
/// 0.5% sliver. We keep active regions 1:1 and shrink dominant idle gaps to a
/// thin fixed width, recording each collapsed gap for the renderer to mark.
struct TimelineMap {
    // (real_start, real_end, disp_start, disp_len)
    segments: Vec<(f64, f64, f64, f64)>,
    display_total: f64,
    trace_start: f64,
    bounding_end: f64,
    gaps: Vec<WaterfallGap>,
    compressed: bool,
    // Set when no span carried a start; callers fall back to flat offsets.
    linear: bool,
}

impl TimelineMap {
    fn build(spans: &[SpanRow], trace_start: i64, bounding_end: i64) -> Self {
        let total_extent = (bounding_end - trace_start).max(1);
        // A gap is worth collapsing only if it dominates the trace and is not tiny.
        let is_big = |gap: i64| gap > total_extent / 4 && gap > 250;

        let mut intervals: Vec<(i64, i64)> = spans
            .iter()
            .filter_map(|s| {
                s.start_ms
                    .map(|st| (st, st.saturating_add(s.duration_ms.unwrap_or(0).max(0))))
            })
            .collect();
        intervals.sort();
        let mut merged: Vec<(i64, i64)> = Vec::new();
        for (a, b) in intervals {
            match merged.last_mut() {
                Some(last) if a <= last.1 => last.1 = last.1.max(b),
                _ => merged.push((a, b)),
            }
        }

        if merged.is_empty() {
            return Self {
                segments: Vec::new(),
                display_total: total_extent as f64,
                trace_start: trace_start as f64,
                bounding_end: bounding_end as f64,
                gaps: Vec::new(),
                compressed: false,
                linear: true,
            };
        }

        let active_total: i64 = merged.iter().map(|(a, b)| b - a).sum();
        let gap_disp = (active_total as f64 * 0.05).max(2.0);

        let mut segments: Vec<(f64, f64, f64, f64)> = Vec::new();
        let mut pending_gaps: Vec<(f64, i64)> = Vec::new(); // (disp_center, real_ms)
        let mut disp = 0.0f64;
        let mut compressed = false;
        let mut prev_end = trace_start;

        // `compressible` gaps sit between two spans; the trailing tail up to the
        // transaction end is uninstrumented work, not idle, so it stays linear.
        let push_gap = |segments: &mut Vec<(f64, f64, f64, f64)>,
                        pending: &mut Vec<(f64, i64)>,
                        disp: &mut f64,
                        compressed: &mut bool,
                        from: i64,
                        to: i64,
                        compressible: bool| {
            let gap = to - from;
            if gap <= 0 {
                return;
            }
            let disp_len = if compressible && is_big(gap) {
                *compressed = true;
                pending.push((*disp + gap_disp / 2.0, gap));
                gap_disp
            } else {
                gap as f64
            };
            segments.push((from as f64, to as f64, *disp, disp_len));
            *disp += disp_len;
        };

        for (a, b) in &merged {
            push_gap(
                &mut segments,
                &mut pending_gaps,
                &mut disp,
                &mut compressed,
                prev_end,
                *a,
                true,
            );
            let alen = (b - a) as f64;
            segments.push((*a as f64, *b as f64, disp, alen));
            disp += alen;
            prev_end = *b;
        }
        // Trailing time (transaction longer than its child spans) stays linear.
        push_gap(
            &mut segments,
            &mut pending_gaps,
            &mut disp,
            &mut compressed,
            prev_end,
            bounding_end,
            false,
        );

        let display_total = disp.max(1.0);
        let gaps = pending_gaps
            .into_iter()
            .map(|(center, real_ms)| WaterfallGap {
                at_pct: (center / display_total * 100.0).clamp(0.0, 100.0),
                real_ms,
            })
            .collect();

        Self {
            segments,
            display_total,
            trace_start: trace_start as f64,
            bounding_end: bounding_end as f64,
            gaps,
            compressed,
            linear: false,
        }
    }

    /// Map a real timestamp to a display percentage in [0, 100].
    fn pct(&self, ms: i64) -> f64 {
        if self.linear {
            let extent = (self.bounding_end - self.trace_start).max(1.0);
            return ((ms as f64 - self.trace_start) / extent * 100.0).clamp(0.0, 100.0);
        }
        let x = (ms as f64).clamp(self.trace_start, self.bounding_end);
        for &(rs, re, ds, dl) in &self.segments {
            if x <= re {
                let span = (re - rs).max(1e-9);
                return ((ds + (x - rs) / span * dl) / self.display_total * 100.0)
                    .clamp(0.0, 100.0);
            }
        }
        100.0
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
    let bounding_end = trace_start.saturating_add(total_ms);
    let timeline = TimelineMap::build(spans, trace_start, bounding_end);

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

        let (offset_pct, mut width_pct) = match s.start_ms {
            Some(st) => {
                let end = st.saturating_add(s.duration_ms.unwrap_or(0).max(0));
                let o = timeline.pct(st);
                (o, (timeline.pct(end) - o).max(0.5))
            }
            None => (0.0, 0.5),
        };
        if offset_pct + width_pct > 100.0 {
            width_pct = 100.0 - offset_pct;
        }

        rows.push(WaterfallRow {
            span_id: s.span_id.clone(),
            parent_span_id: s.parent_span_id.clone(),
            depth: depth.min(MAX_DEPTH),
            op: s.op.clone(),
            description: s.description.clone(),
            status: s.status.clone(),
            duration_ms: s.duration_ms,
            start_offset_ms: s.start_ms.map(|st| st - trace_start),
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
        gaps: timeline.gaps,
        compressed: timeline.compressed,
    }
}

pub async fn list_traces(
    pool: &crate::db::DbPool,
    project_id: u64,
    page: &Page,
) -> Result<PagedResult<TraceSummary>> {
    list_traces_with_scan_limit(pool, project_id, page, SPAN_AGG_SCAN_LIMIT).await
}

async fn list_traces_with_scan_limit(
    pool: &crate::db::DbPool,
    project_id: u64,
    page: &Page,
    scan_limit: i64,
) -> Result<PagedResult<TraceSummary>> {
    // Count over the same recency window as the listing so pagination stays consistent with what the bounded scan can return.
    let count_row = sqlx::query(sql!(
        "SELECT COUNT(DISTINCT trace_id) FROM (
            SELECT trace_id FROM spans
            WHERE project_id = ?1 AND trace_id IS NOT NULL
            ORDER BY timestamp DESC
            LIMIT ?2) recent"
    ))
    .bind(project_id as i64)
    .bind(scan_limit)
    .fetch_one(pool)
    .await?;
    let total = count_row.get::<i64, _>(0);

    let rows = sqlx::query(sql!(
        "SELECT trace_id,
                COUNT(*) AS span_count,
                MIN(timestamp) AS first_timestamp,
                MAX(timestamp) AS last_timestamp,
                MAX(start_ms + COALESCE(duration_ms, 0)) - MIN(start_ms) AS span_extent_ms
         FROM (SELECT trace_id, timestamp, start_ms, duration_ms
               FROM spans
               WHERE project_id = ?1 AND trace_id IS NOT NULL
               ORDER BY timestamp DESC
               LIMIT ?2) recent
         GROUP BY trace_id
         ORDER BY last_timestamp DESC
         LIMIT ?3 OFFSET ?4"
    ))
    .bind(project_id as i64)
    .bind(scan_limit)
    .bind(page.limit as i64)
    .bind(page.offset as i64)
    .fetch_all(pool)
    .await?;

    let mut items: Vec<TraceSummary> = Vec::with_capacity(rows.len());
    for row in &rows {
        let span_extent_ms = row
            .get::<Option<i64>, _>("span_extent_ms")
            .unwrap_or(0)
            .max(0);
        items.push(TraceSummary {
            trace_id: row.get_opt_string("trace_id").unwrap_or_default(),
            span_count: row.get_u64("span_count"),
            first_timestamp: row.get("first_timestamp"),
            last_timestamp: row.get("last_timestamp"),
            root_op: None,
            root_description: None,
            total_duration_ms: Some(span_extent_ms),
        });
    }

    // transaction_name fallback for root_description when no stored root span supplies one.
    let mut fallback_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    if !items.is_empty() {
        // One batched lookup for the page's owning transactions (duration + name)
        // instead of correlated subqueries inside the GROUP BY over all spans.
        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
            "SELECT trace_id, MAX(duration_ms) AS root_duration_ms, MAX(transaction_name) AS root_txn_name
             FROM events WHERE item_type = 'transaction' AND trace_id IN (",
        );
        let mut sep = qb.separated(", ");
        for item in &items {
            sep.push_bind(item.trace_id.clone());
        }
        qb.push(") AND project_id = ");
        qb.push_bind(project_id as i64);
        qb.push(" GROUP BY trace_id");
        let txn_meta_rows = qb.build().fetch_all(pool).await?;

        let mut duration_map: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        for row in &txn_meta_rows {
            let tid: String = row.get_opt_string("trace_id").unwrap_or_default();
            if let Some(d) = row.get::<Option<i64>, _>("root_duration_ms") {
                duration_map.insert(tid.clone(), d);
            }
            if let Some(name) = row.get_opt_string("root_txn_name") {
                fallback_names.insert(tid, name);
            }
        }

        for item in &mut items {
            if let Some(root_duration_ms) = duration_map.remove(&item.trace_id) {
                item.total_duration_ms = item.total_duration_ms.map(|e| e.max(root_duration_ms));
            }
        }
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
            let tid: String = row.get_opt_string("trace_id").unwrap_or_default();
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
            let tid: String = row.get_opt_string("trace_id").unwrap_or_default();
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
            parent_span_id: None,
            depth: 0,
            op: None,
            description: None,
            status: status.map(String::from),
            duration_ms: Some(1),
            start_offset_ms: Some(0),
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
    fn dense_trace_is_not_compressed() {
        // Two adjacent spans, no dominant idle gap.
        let spans = vec![
            span("a", None, Some(0), Some(100)),
            span("b", None, Some(120), Some(80)),
        ];
        let w = build_waterfall(&spans, 0);
        assert!(!w.compressed);
        assert!(w.gaps.is_empty());
        // Header total stays the real extent.
        assert_eq!(w.total_ms, 200);
    }

    #[test]
    fn idle_gap_is_compressed_and_early_span_stays_visible() {
        // An early 100ms span, then a huge idle gap, then a late 10ms span.
        let spans = vec![
            span("early", None, Some(0), Some(100)),
            span("late", None, Some(40_000), Some(10)),
        ];
        let w = build_waterfall(&spans, 0);
        assert!(w.compressed, "dominant idle gap should compress");
        assert_eq!(w.gaps.len(), 1);
        assert_eq!(w.gaps[0].real_ms, 39_900);
        // Real total is preserved for the header.
        assert_eq!(w.total_ms, 40_010);

        // Under the old linear scale the early span would be 100/40010 ≈ 0.25%.
        // Compression must make it clearly visible.
        let early = row(&w, "early");
        assert!(
            early.width_pct > 5.0,
            "early span crushed: {}%",
            early.width_pct
        );
        // The late span still sits to the right of the early one.
        let late = row(&w, "late");
        assert!(late.offset_pct > early.offset_pct);
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

    #[allow(clippy::too_many_arguments)]
    async fn insert_span(
        pool: &crate::db::DbPool,
        span_id: &str,
        trace_id: &str,
        parent: Option<&str>,
        project_id: i64,
        timestamp: i64,
        start_ms: i64,
        duration_ms: i64,
        op: Option<&str>,
        description: Option<&str>,
    ) {
        let compressed = zstd::encode_all([0u8; 0].as_slice(), 3).unwrap();
        sqlx::query(sql!(
            "INSERT INTO spans (span_id, payload, project_id, public_key, timestamp, trace_id, parent_span_id, op, description, status, duration_ms, start_ms)
             VALUES (?1, ?2, ?3, 'testkey', ?4, ?5, ?6, ?7, ?8, 'ok', ?9, ?10)"
        ))
        .bind(span_id)
        .bind(&compressed)
        .bind(project_id)
        .bind(timestamp)
        .bind(trace_id)
        .bind(parent)
        .bind(op)
        .bind(description)
        .bind(duration_ms)
        .bind(start_ms)
        .execute(pool)
        .await
        .unwrap();
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_txn(
        pool: &crate::db::DbPool,
        event_id: &str,
        project_id: i64,
        trace_id: &str,
        timestamp: i64,
        transaction_name: &str,
        duration_ms: i64,
        op: &str,
    ) {
        let payload = serde_json::json!({ "contexts": { "trace": { "op": op } } });
        let compressed =
            zstd::encode_all(serde_json::to_vec(&payload).unwrap().as_slice(), 3).unwrap();
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, trace_id, transaction_name, duration_ms, level)
             VALUES (?1, 'transaction', ?2, ?3, 'testkey', ?4, ?5, ?6, ?7, 'info')"
        ))
        .bind(event_id)
        .bind(&compressed)
        .bind(project_id)
        .bind(timestamp)
        .bind(trace_id)
        .bind(transaction_name)
        .bind(duration_ms)
        .execute(pool)
        .await
        .unwrap();
    }

    // Guards the feature-spans fixes: with no stored root span, the trace row's
    // op/description must fall back to the owning transaction (name -> description,
    // payload contexts.trace.op -> root_op) instead of rendering blank, and the
    // duration must follow the transaction when it exceeds the child-span extent.
    #[tokio::test]
    async fn list_traces_root_falls_back_to_transaction_name_and_payload_op() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        // Child spans only (both orphaned), so no parent_span_id IS NULL root span.
        // Child extent is 0..400ms = 400ms.
        insert_span(
            &pool,
            "s1",
            "t1",
            Some("ghost"),
            1,
            100,
            0,
            200,
            Some("db.query"),
            Some("SELECT 1"),
        )
        .await;
        insert_span(
            &pool,
            "s2",
            "t1",
            Some("ghost"),
            1,
            100,
            100,
            300,
            None,
            None,
        )
        .await;
        // Owning transaction lasts 1000ms.
        insert_txn(
            &pool,
            "tx1",
            1,
            "t1",
            110,
            "GET /checkout",
            1000,
            "http.server",
        )
        .await;

        let page = Page {
            offset: 0,
            limit: 50,
        };
        let res = list_traces(&pool, 1, &page).await.unwrap();
        assert_eq!(res.items.len(), 1);
        let t = &res.items[0];
        assert_eq!(t.trace_id, "t1");
        assert_eq!(t.span_count, 2);
        // Duration follows the 1000ms transaction, not the 400ms child extent.
        assert_eq!(t.total_duration_ms, Some(1000));
        assert_eq!(t.root_description.as_deref(), Some("GET /checkout"));
        assert_eq!(t.root_op.as_deref(), Some("http.server"));
    }

    // The trace listing (count + GROUP BY) must only scan the newest
    // `scan_limit` spans, so older traces fall out of both the page and the total.
    #[tokio::test]
    async fn list_traces_scan_limit_bounds_to_newest_spans() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        insert_span(&pool, "o1", "t_old", None, 1, 100, 0, 10, None, None).await;
        insert_span(&pool, "o2", "t_old", None, 1, 101, 0, 10, None, None).await;
        insert_span(&pool, "n1", "t_new", None, 1, 200, 0, 10, None, None).await;
        insert_span(&pool, "n2", "t_new", None, 1, 201, 0, 10, None, None).await;

        let page = Page {
            offset: 0,
            limit: 50,
        };
        let bounded = list_traces_with_scan_limit(&pool, 1, &page, 2)
            .await
            .unwrap();
        assert_eq!(bounded.total, 1, "count must respect the scan bound");
        assert_eq!(bounded.items.len(), 1);
        assert_eq!(bounded.items[0].trace_id, "t_new");
        assert_eq!(bounded.items[0].span_count, 2);

        let unbounded = list_traces_with_scan_limit(&pool, 1, &page, 1000)
            .await
            .unwrap();
        assert_eq!(unbounded.total, 2);
        assert_eq!(unbounded.items[0].trace_id, "t_new");
        assert_eq!(unbounded.items[1].trace_id, "t_old");
    }

    #[test]
    fn span_agg_percentiles_and_count() {
        // p50 nearest-rank of 1..=10 = ceil(0.5*10)=5th -> 5; p95 = ceil(0.95*10)=10th -> 10.
        let durations = vec![10, 1, 5, 3, 8, 2, 9, 4, 7, 6];
        let agg = build_span_agg(Some("db.query".into()), Some("SELECT 1".into()), durations);
        assert_eq!(agg.count, 10);
        assert_eq!(agg.op.as_deref(), Some("db.query"));
        assert_eq!(agg.p50_ms, 5);
        assert_eq!(agg.p95_ms, 10);
        assert_eq!(agg.avg_ms, 5); // (55 / 10) truncated

        // Single sample: every percentile is that sample.
        let one = build_span_agg(None, None, vec![42]);
        assert_eq!(one.count, 1);
        assert_eq!(one.p50_ms, 42);
        assert_eq!(one.p95_ms, 42);
        assert_eq!(one.avg_ms, 42);

        // Empty group degrades to zeros without panicking.
        let empty = build_span_agg(None, None, vec![]);
        assert_eq!(empty.count, 0);
        assert_eq!(empty.p50_ms, 0);
        assert_eq!(empty.p95_ms, 0);
        assert_eq!(empty.avg_ms, 0);
    }

    #[tokio::test]
    async fn aggregate_spans_groups_by_op_description() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        // Group A: db.query / SELECT 1 -> durations 10, 20, 30.
        insert_span(
            &pool,
            "a1",
            "t1",
            None,
            1,
            100,
            0,
            10,
            Some("db.query"),
            Some("SELECT 1"),
        )
        .await;
        insert_span(
            &pool,
            "a2",
            "t1",
            None,
            1,
            101,
            0,
            20,
            Some("db.query"),
            Some("SELECT 1"),
        )
        .await;
        insert_span(
            &pool,
            "a3",
            "t1",
            None,
            1,
            102,
            0,
            30,
            Some("db.query"),
            Some("SELECT 1"),
        )
        .await;
        // Group B: http.client / GET -> single duration 5.
        insert_span(
            &pool,
            "b1",
            "t1",
            None,
            1,
            103,
            0,
            5,
            Some("http.client"),
            Some("GET"),
        )
        .await;
        // NULL-duration span must be skipped entirely.
        sqlx::query(sql!(
            "INSERT INTO spans (span_id, payload, project_id, public_key, timestamp, op, description)
             VALUES ('c1', ?1, 1, 'testkey', 104, 'noop', 'x')"
        ))
        .bind(zstd::encode_all([0u8; 0].as_slice(), 3).unwrap())
        .execute(&pool)
        .await
        .unwrap();

        let agg = aggregate_spans(&pool, 1).await.unwrap();
        assert!(!agg.truncated);
        assert_eq!(agg.groups.len(), 2);
        // Sorted count desc: the 3-sample db.query group leads.
        let a = &agg.groups[0];
        assert_eq!(a.op.as_deref(), Some("db.query"));
        assert_eq!(a.count, 3);
        assert_eq!(a.p50_ms, 20);
        assert_eq!(a.p95_ms, 30);
        assert_eq!(a.avg_ms, 20);
        let b = &agg.groups[1];
        assert_eq!(b.op.as_deref(), Some("http.client"));
        assert_eq!(b.count, 1);
        assert_eq!(b.p95_ms, 5);
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

    /// A trace id is shared across projects in a distributed trace. The web
    /// trace view authorized the project but read spans by trace id alone, so a
    /// known id exposed another org's spans.
    #[tokio::test]
    async fn trace_spans_never_cross_a_project_boundary() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        insert_span(
            &pool,
            "mine",
            "shared-trace",
            None,
            1,
            100,
            0,
            100,
            Some("http.server"),
            Some("GET /mine"),
        )
        .await;
        insert_span(
            &pool,
            "theirs",
            "shared-trace",
            None,
            2,
            100,
            0,
            100,
            Some("http.server"),
            Some("GET /theirs"),
        )
        .await;

        let mine = get_trace_spans_for_project(&pool, 1, "shared-trace")
            .await
            .unwrap();
        assert_eq!(mine.len(), 1, "only the caller's project");
        assert_eq!(mine[0].span_id, "mine");

        let theirs = get_trace_spans_for_project(&pool, 2, "shared-trace")
            .await
            .unwrap();
        assert_eq!(theirs.len(), 1);
        assert_eq!(theirs[0].span_id, "theirs");
    }
}
