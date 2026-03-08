use anyhow::Result;
use sqlx::Row;

use crate::db::sql;

use super::types::{Page, PagedResult, SpanSummary, TraceSpan, TraceSummary};

pub async fn list_spans(
    pool: &crate::db::DbPool,
    project_id: u64,
    page: &Page,
) -> Result<PagedResult<SpanSummary>> {
    let count_row = sqlx::query(sql!("SELECT COUNT(*) FROM spans WHERE project_id = ?1"))
        .bind(project_id as i64)
        .fetch_one(pool)
        .await?;
    let total = count_row.get::<i64, _>(0) as u64;

    let rows = sqlx::query(sql!(
        "SELECT span_id, trace_id, parent_span_id, timestamp, op, description, status, duration_ms
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
            parent_span_id: row.get("parent_span_id"),
            timestamp: row.get("timestamp"),
            op: row.get("op"),
            description: row.get("description"),
            status: row.get("status"),
            duration_ms: row.get("duration_ms"),
        })
        .collect();

    Ok(PagedResult {
        items,
        total,
        offset: page.offset,
        limit: page.limit,
    })
}

pub async fn get_trace_spans(pool: &crate::db::DbPool, trace_id: &str) -> Result<Vec<TraceSpan>> {
    let rows = sqlx::query(sql!(
        "SELECT span_id, trace_id, parent_span_id, timestamp, op, description, status, duration_ms
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
            trace_id: row.get::<Option<String>, _>("trace_id").unwrap_or_default(),
            parent_span_id: row.get("parent_span_id"),
            timestamp: row.get("timestamp"),
            op: row.get("op"),
            description: row.get("description"),
            status: row.get("status"),
            duration_ms: row.get("duration_ms"),
        })
        .collect())
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
    let total = count_row.get::<i64, _>(0) as u64;

    let rows = sqlx::query(sql!(
        "SELECT trace_id,
                COUNT(*) AS span_count,
                MIN(timestamp) AS first_timestamp,
                MAX(timestamp) AS last_timestamp,
                (MAX(timestamp) - MIN(timestamp)) * 1000 + MAX(COALESCE(duration_ms, 0)) AS total_duration_ms
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

    let mut items: Vec<TraceSummary> = rows
        .iter()
        .map(|row| TraceSummary {
            trace_id: row.get::<Option<String>, _>("trace_id").unwrap_or_default(),
            span_count: row.get::<i64, _>("span_count") as u64,
            first_timestamp: row.get("first_timestamp"),
            last_timestamp: row.get("last_timestamp"),
            root_op: None,
            root_description: None,
            total_duration_ms: row.get("total_duration_ms"),
        })
        .collect();

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

    Ok(PagedResult {
        items,
        total,
        offset: page.offset,
        limit: page.limit,
    })
}
