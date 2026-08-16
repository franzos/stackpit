//! Notifications that failed delivery and are waiting to be retried.

use crate::db::{sql, DbPool};
use anyhow::Result;
use sqlx::Row;

pub const STATUS_PENDING: &str = "pending";
/// Past the retry window: kept for the UI, retried only on an explicit replay.
pub const STATUS_FAILED: &str = "failed";

pub struct QueuedDelivery {
    pub id: i64,
    pub org_id: i64,
    pub project_id: i64,
    pub integration_id: i64,
    /// The serialised `NotificationEvent`.
    pub payload: String,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub next_attempt_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

const SELECT: &str = "SELECT id, org_id, project_id, integration_id, payload, status,
            attempts, last_error, next_attempt_at, created_at, updated_at
     FROM notification_delivery_queue";

fn row_to_item(row: &crate::db::DbRow) -> QueuedDelivery {
    QueuedDelivery {
        id: row.get("id"),
        org_id: row.get("org_id"),
        project_id: row.get("project_id"),
        integration_id: row.get("integration_id"),
        payload: row.get("payload"),
        status: row.get("status"),
        attempts: row.get("attempts"),
        last_error: row.get("last_error"),
        next_attempt_at: row.get("next_attempt_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

/// `attempts` starts at 1: the in-process try and its retry already happened.
#[allow(clippy::too_many_arguments)]
pub async fn enqueue(
    pool: &DbPool,
    org_id: i64,
    project_id: i64,
    integration_id: i64,
    payload: &str,
    error: &str,
    now: i64,
    next_attempt_at: i64,
) -> Result<()> {
    sqlx::query(sql!(
        "INSERT INTO notification_delivery_queue
             (org_id, project_id, integration_id, payload, status, attempts,
              last_error, next_attempt_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, ?8, ?8)"
    ))
    .bind(org_id)
    .bind(project_id)
    .bind(integration_id)
    .bind(payload)
    .bind(STATUS_PENDING)
    .bind(error)
    .bind(next_attempt_at)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Pending items whose backoff has elapsed, oldest first.
pub async fn due(pool: &DbPool, now: i64, limit: i64) -> Result<Vec<QueuedDelivery>> {
    let sql = format!(
        "{SELECT} WHERE status = ?1 AND next_attempt_at <= ?2 ORDER BY next_attempt_at, id LIMIT ?3"
    );
    let rows = sqlx::query(crate::db::dyn_sql(&sql))
        .bind(STATUS_PENDING)
        .bind(now)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().map(row_to_item).collect())
}

pub async fn list_for_org(pool: &DbPool, org_id: i64, limit: i64) -> Result<Vec<QueuedDelivery>> {
    let sql = format!("{SELECT} WHERE org_id = ?1 ORDER BY id DESC LIMIT ?2");
    let rows = sqlx::query(crate::db::dyn_sql(&sql))
        .bind(org_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().map(row_to_item).collect())
}

/// Queue item plus display names - the project may be gone, the integration can't (rows cascade).
pub struct QueuedDeliveryView {
    pub item: QueuedDelivery,
    pub integration_name: String,
    pub integration_kind: String,
    pub project_name: Option<String>,
}

pub async fn list_for_org_detailed(
    pool: &DbPool,
    org_id: i64,
    limit: i64,
) -> Result<Vec<QueuedDeliveryView>> {
    let rows = sqlx::query(sql!(
        "SELECT q.id, q.org_id, q.project_id, q.integration_id, q.payload, q.status,
                q.attempts, q.last_error, q.next_attempt_at, q.created_at, q.updated_at,
                i.name AS integration_name, i.kind AS integration_kind, p.name AS project_name
         FROM notification_delivery_queue q
         JOIN integrations i ON i.id = q.integration_id
         LEFT JOIN projects p ON p.project_id = q.project_id
         WHERE q.org_id = ?1 ORDER BY q.id DESC LIMIT ?2"
    ))
    .bind(org_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| QueuedDeliveryView {
            item: row_to_item(r),
            integration_name: r.get("integration_name"),
            integration_kind: r.get("integration_kind"),
            project_name: r.get("project_name"),
        })
        .collect())
}

/// Org-scoped so a forged id can't reach another org's queue.
pub async fn get(pool: &DbPool, id: i64, org_id: i64) -> Result<Option<QueuedDelivery>> {
    let sql = format!("{SELECT} WHERE id = ?1 AND org_id = ?2");
    let row = sqlx::query(crate::db::dyn_sql(&sql))
        .bind(id)
        .bind(org_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.as_ref().map(row_to_item))
}

pub async fn record_attempt(
    pool: &DbPool,
    id: i64,
    error: &str,
    now: i64,
    next_attempt_at: i64,
    status: &str,
) -> Result<u64> {
    let result = sqlx::query(sql!(
        "UPDATE notification_delivery_queue
         SET attempts = attempts + 1, last_error = ?2, next_attempt_at = ?3,
             status = ?4, updated_at = ?5
         WHERE id = ?1"
    ))
    .bind(id)
    .bind(error)
    .bind(next_attempt_at)
    .bind(status)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// `org_id` is `None` only for the drain, which already resolved the row.
pub async fn delete(pool: &DbPool, id: i64, org_id: Option<i64>) -> Result<u64> {
    let result = match org_id {
        Some(oid) => {
            sqlx::query(sql!(
                "DELETE FROM notification_delivery_queue WHERE id = ?1 AND org_id = ?2"
            ))
            .bind(id)
            .bind(oid)
            .execute(pool)
            .await?
        }
        None => {
            sqlx::query(sql!(
                "DELETE FROM notification_delivery_queue WHERE id = ?1"
            ))
            .bind(id)
            .execute(pool)
            .await?
        }
    };
    Ok(result.rows_affected())
}

pub async fn purge_failed_before(pool: &DbPool, cutoff: i64) -> Result<u64> {
    let result = sqlx::query(sql!(
        "DELETE FROM notification_delivery_queue WHERE status = ?1 AND updated_at < ?2"
    ))
    .bind(STATUS_FAILED)
    .bind(cutoff)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Keep at most `max_rows` per integration, oldest dropped first.
pub async fn trim_per_integration(pool: &DbPool, max_rows: i64) -> Result<u64> {
    let result = sqlx::query(sql!(
        "DELETE FROM notification_delivery_queue
         WHERE id IN (
             SELECT id FROM (
                 SELECT id, ROW_NUMBER() OVER (
                     PARTITION BY integration_id ORDER BY id DESC
                 ) AS rn
                 FROM notification_delivery_queue
             ) ranked
             WHERE rn > ?1
         )"
    ))
    .bind(max_rows)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Returns (pending, failed).
pub async fn counts_for_org(pool: &DbPool, org_id: i64) -> Result<(i64, i64)> {
    let row = sqlx::query(sql!(
        "SELECT
             SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) AS pending,
             SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) AS failed
         FROM notification_delivery_queue WHERE org_id = ?1"
    ))
    .bind(org_id)
    .fetch_one(pool)
    .await?;
    Ok((
        row.get::<Option<i64>, _>("pending").unwrap_or(0),
        row.get::<Option<i64>, _>("failed").unwrap_or(0),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::test_helpers::open_test_db;

    async fn seed_integration(pool: &DbPool, org_id: i64, name: &str) -> i64 {
        sqlx::query(sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (?1, ?2, ?2)
             ON CONFLICT (org_id) DO NOTHING"
        ))
        .bind(org_id)
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .unwrap();
        crate::queries::integrations::create_integration(
            pool,
            org_id,
            name,
            "webhook",
            Some("https://hooks.test/x"),
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn an_item_round_trips_and_only_comes_due_once_its_backoff_elapses() {
        let pool = open_test_db().await;
        let id = seed_integration(&pool, 5, "hooks").await;
        enqueue(&pool, 5, 42, id, r#"{"a":1}"#, "boom", 1000, 1030)
            .await
            .unwrap();

        assert!(
            due(&pool, 1029, 10).await.unwrap().is_empty(),
            "still backing off"
        );
        let items = due(&pool, 1030, 10).await.unwrap();
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.payload, r#"{"a":1}"#);
        assert_eq!(
            item.attempts, 1,
            "the in-process try and its retry already happened"
        );
        assert_eq!(item.last_error.as_deref(), Some("boom"));
        assert_eq!(item.status, STATUS_PENDING);

        assert_eq!(counts_for_org(&pool, 5).await.unwrap(), (1, 0));

        record_attempt(&pool, item.id, "boom again", 1030, 1090, STATUS_PENDING)
            .await
            .unwrap();
        let after = get(&pool, item.id, 5).await.unwrap().unwrap();
        assert_eq!(after.attempts, 2);
        assert_eq!(after.next_attempt_at, 1090);

        record_attempt(&pool, item.id, "gave up", 1090, 1090, STATUS_FAILED)
            .await
            .unwrap();
        assert!(
            due(&pool, 9999, 10).await.unwrap().is_empty(),
            "a parked item is not due"
        );
        assert_eq!(counts_for_org(&pool, 5).await.unwrap(), (0, 1));
    }

    #[tokio::test]
    async fn reads_and_deletes_are_org_scoped() {
        let pool = open_test_db().await;
        let id = seed_integration(&pool, 5, "hooks-scoped").await;
        enqueue(&pool, 5, 42, id, "{}", "boom", 1000, 1000)
            .await
            .unwrap();
        let item = due(&pool, 1000, 10).await.unwrap().pop().unwrap();

        assert!(get(&pool, item.id, 6).await.unwrap().is_none());
        assert!(list_for_org(&pool, 6, 50).await.unwrap().is_empty());
        assert_eq!(delete(&pool, item.id, Some(6)).await.unwrap(), 0);
        assert!(get(&pool, item.id, 5).await.unwrap().is_some());
        assert_eq!(delete(&pool, item.id, Some(5)).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn queued_items_go_with_their_integration() {
        let pool = open_test_db().await;
        let id = seed_integration(&pool, 5, "hooks-gone").await;
        enqueue(&pool, 5, 42, id, "{}", "boom", 1000, 1000)
            .await
            .unwrap();

        crate::queries::integrations::delete_integration(&pool, id, 5)
            .await
            .unwrap();
        assert!(list_for_org(&pool, 5, 50).await.unwrap().is_empty());
    }
}
