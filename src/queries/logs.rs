use anyhow::Result;
use sqlx::Row;

use super::types::{LogEntry, LogFilter, Page, PagedResult};

fn push_log_filter_conditions<'args>(
    qb: &mut sqlx::QueryBuilder<'args, crate::db::Db>,
    project_id: u64,
    filter: &'args LogFilter,
) {
    qb.push(" WHERE project_id = ");
    qb.push_bind(project_id as i64);

    if let Some(ref level) = filter.level {
        qb.push(" AND level = ");
        qb.push_bind(level.as_str());
    }
    if let Some(ref trace_id) = filter.trace_id {
        qb.push(" AND trace_id = ");
        qb.push_bind(trace_id.as_str());
    }
    if let Some(ref query) = filter.query {
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        qb.push(" AND body LIKE ");
        qb.push_bind(format!("%{escaped}%"));
        qb.push(" ESCAPE '\\'");
    }
}

pub async fn list_logs(
    pool: &crate::db::DbPool,
    project_id: u64,
    filter: &LogFilter,
    page: &Page,
) -> Result<PagedResult<LogEntry>> {
    use sqlx::QueryBuilder;

    let mut count_qb: QueryBuilder<'_, crate::db::Db> =
        QueryBuilder::new("SELECT COUNT(*) FROM logs");
    push_log_filter_conditions(&mut count_qb, project_id, filter);

    let total: i64 = count_qb.build_query_scalar().fetch_one(pool).await?;

    let mut select_qb: QueryBuilder<'_, crate::db::Db> = QueryBuilder::new(
        "SELECT id, project_id, timestamp, level, body, trace_id, span_id, release, environment, attributes FROM logs",
    );
    push_log_filter_conditions(&mut select_qb, project_id, filter);
    select_qb.push(" ORDER BY timestamp DESC LIMIT ");
    select_qb.push_bind(page.limit as i64);
    select_qb.push(" OFFSET ");
    select_qb.push_bind(page.offset as i64);

    let rows = select_qb.build().fetch_all(pool).await?;
    let items: Vec<LogEntry> = rows.iter().map(map_log_row).collect();

    Ok(PagedResult {
        items,
        total: total as u64,
        offset: page.offset,
        limit: page.limit,
    })
}

fn map_log_row(row: &crate::db::DbRow) -> LogEntry {
    LogEntry {
        id: row.get("id"),
        project_id: row.get::<i64, _>("project_id") as u64,
        timestamp: row.get("timestamp"),
        level: row.get("level"),
        body: row.get("body"),
        trace_id: row.get("trace_id"),
        span_id: row.get("span_id"),
        release: row.get("release"),
        environment: row.get("environment"),
        attributes: row.get("attributes"),
    }
}
