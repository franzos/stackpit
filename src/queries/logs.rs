use anyhow::Result;
use sqlx::Row;

use crate::db::DbRowExt;

use super::types::{LogEntry, LogFilter, Page, PagedResult};

fn push_log_filter_conditions(
    qb: &mut sqlx::QueryBuilder<crate::db::Db>,
    project_id: u64,
    filter: &LogFilter,
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
        qb.push(" AND body LIKE ");
        qb.push_bind(super::like_contains(query));
        qb.push(" ESCAPE '\\'");
    }
    if let Some(since_ts) = filter.since_ts {
        qb.push(" AND timestamp >= ");
        qb.push_bind(since_ts);
    }
}

pub async fn list_logs(
    pool: &crate::db::DbPool,
    project_id: u64,
    filter: &LogFilter,
    page: &Page,
) -> Result<PagedResult<LogEntry>> {
    use sqlx::QueryBuilder;

    let mut count_qb: QueryBuilder<crate::db::Db> = QueryBuilder::new("SELECT COUNT(*) FROM logs");
    push_log_filter_conditions(&mut count_qb, project_id, filter);

    let total: i64 = count_qb.build_query_scalar().fetch_one(pool).await?;

    let mut select_qb: QueryBuilder<crate::db::Db> = QueryBuilder::new(
        "SELECT id, project_id, timestamp, level, body, trace_id, span_id, release, environment, attributes FROM logs",
    );
    push_log_filter_conditions(&mut select_qb, project_id, filter);
    select_qb.push(" ORDER BY timestamp DESC LIMIT ");
    select_qb.push_bind(page.limit as i64);
    select_qb.push(" OFFSET ");
    select_qb.push_bind(page.offset as i64);

    let rows = select_qb.build().fetch_all(pool).await?;
    let items: Vec<LogEntry> = rows.iter().map(map_log_row).collect();

    Ok(PagedResult::from_page(items, total, page))
}

fn map_log_row(row: &crate::db::DbRow) -> LogEntry {
    LogEntry {
        id: row.get("id"),
        project_id: row.get_u64("project_id"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sql;

    async fn insert_log(pool: &crate::db::DbPool, project_id: i64, ts: i64, body: &str) {
        sqlx::query(sql!(
            "INSERT INTO logs (payload, project_id, public_key, timestamp, level, body)
             VALUES (?1, ?2, 'k', ?3, 'error', ?4)"
        ))
        .bind(Vec::<u8>::new())
        .bind(project_id)
        .bind(ts)
        .bind(body)
        .execute(pool)
        .await
        .unwrap();
    }

    // The count and the page have to agree on the window, or the pager offers
    // pages the listing cannot fill.
    #[tokio::test]
    async fn list_logs_honours_the_time_window() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        insert_log(&pool, 1, 100, "old").await;
        insert_log(&pool, 1, 900, "new").await;

        let page = Page::new(Some(0), Some(25));

        let all = list_logs(&pool, 1, &LogFilter::default(), &page)
            .await
            .unwrap();
        assert_eq!(all.total, 2);

        let filter = LogFilter {
            since_ts: Some(500),
            ..Default::default()
        };
        let windowed = list_logs(&pool, 1, &filter, &page).await.unwrap();
        assert_eq!(windowed.total, 1);
        assert_eq!(windowed.items[0].body.as_deref(), Some("new"));
    }
}
