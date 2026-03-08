use anyhow::Result;
use sqlx::Row;

use crate::db::{sql, DbPool};

use super::types::{MetricBucket, MetricInfo, Page, PagedResult};

pub async fn list_metrics(
    pool: &DbPool,
    project_id: u64,
    page: &Page,
) -> Result<PagedResult<MetricInfo>> {
    #[cfg(feature = "sqlite")]
    let count_sql = sql!("SELECT COUNT(*) FROM (SELECT 1 FROM metrics WHERE project_id = ?1 GROUP BY mri, metric_type)");
    #[cfg(not(feature = "sqlite"))]
    let count_sql = sql!("SELECT COUNT(*) FROM (SELECT 1 FROM metrics WHERE project_id = ?1 GROUP BY mri, metric_type) AS sub");

    let count_row = sqlx::query(count_sql)
        .bind(project_id as i64)
        .fetch_one(pool)
        .await?;
    let total = count_row.get::<i64, _>(0) as u64;

    let rows = sqlx::query(sql!(
        "SELECT mri, metric_type, COUNT(*) AS data_points, MIN(timestamp) AS first_seen, MAX(timestamp) AS last_seen
         FROM metrics WHERE project_id = ?1
         GROUP BY mri, metric_type
         ORDER BY last_seen DESC
         LIMIT ?2 OFFSET ?3"
    ))
    .bind(project_id as i64)
    .bind(page.limit as i64)
    .bind(page.offset as i64)
    .fetch_all(pool)
    .await?;

    let items = rows
        .iter()
        .map(|row| MetricInfo {
            mri: row.get("mri"),
            metric_type: row.get("metric_type"),
            data_points: row.get::<i64, _>("data_points") as u64,
            first_seen: row.get("first_seen"),
            last_seen: row.get("last_seen"),
        })
        .collect();

    Ok(PagedResult {
        items,
        total,
        offset: page.offset,
        limit: page.limit,
    })
}

pub async fn get_metric_type(pool: &DbPool, project_id: u64, mri: &str) -> Option<String> {
    sqlx::query(sql!(
        "SELECT metric_type FROM metrics WHERE project_id = ?1 AND mri = ?2 LIMIT 1"
    ))
    .bind(project_id as i64)
    .bind(mri)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|row| row.get("metric_type"))
}

pub async fn get_metric_series(
    pool: &DbPool,
    project_id: u64,
    mri: &str,
    from: Option<i64>,
    to: Option<i64>,
) -> Result<Vec<MetricBucket>> {
    let now = chrono::Utc::now().timestamp();
    let from_ts = from.unwrap_or(now - 7 * 86400);
    let to_ts = to.unwrap_or(now);

    #[cfg(feature = "sqlite")]
    let bucket_sql = sql!(
        "SELECT (timestamp / 3600) * 3600 AS bucket,
                COUNT(*) AS count,
                SUM(value) AS sum,
                MIN(value) AS min,
                MAX(value) AS max,
                AVG(value) AS avg
         FROM metrics
         WHERE project_id = ?1 AND mri = ?2 AND timestamp >= ?3 AND timestamp <= ?4
         GROUP BY bucket
         ORDER BY bucket
         LIMIT 8760"
    );
    #[cfg(not(feature = "sqlite"))]
    let bucket_sql = sql!(
        "SELECT EXTRACT(EPOCH FROM date_trunc('hour', to_timestamp(timestamp)))::BIGINT AS bucket,
                COUNT(*) AS count,
                SUM(value) AS sum,
                MIN(value) AS min,
                MAX(value) AS max,
                AVG(value) AS avg
         FROM metrics
         WHERE project_id = ?1 AND mri = ?2 AND timestamp >= ?3 AND timestamp <= ?4
         GROUP BY bucket
         ORDER BY bucket
         LIMIT 8760"
    );

    let rows = sqlx::query(bucket_sql)
        .bind(project_id as i64)
        .bind(mri)
        .bind(from_ts)
        .bind(to_ts)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .iter()
        .map(|row| MetricBucket {
            timestamp: row.get::<i64, _>("bucket"),
            count: row.get::<i64, _>("count") as u64,
            sum: row.get::<f64, _>("sum"),
            min: row.get::<f64, _>("min"),
            max: row.get::<f64, _>("max"),
            avg: row.get::<f64, _>("avg"),
        })
        .collect())
}
