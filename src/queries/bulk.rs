use anyhow::Result;
use sqlx::Row;

use super::types::{EventFilter, IssueFilter, IssueStatus};

use crate::db::DbPool;

/// Resolve a filter down to a list of fingerprints using QueryBuilder.
async fn fingerprints_by_filter(
    pool: &DbPool,
    project_id: u64,
    filter: &IssueFilter,
    since: Option<i64>,
) -> Result<Vec<String>> {
    let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
        "SELECT fingerprint FROM issues WHERE project_id = ",
    );
    qb.push_bind(project_id as i64);

    if let Some(ref level) = filter.level {
        qb.push(" AND level = ");
        qb.push_bind(level.as_str());
    }
    if let Some(ref status) = filter.status {
        qb.push(" AND status = ");
        qb.push_bind(status.as_str());
    }
    if let Some(ref query) = filter.query {
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        qb.push(" AND title LIKE ");
        qb.push_bind(format!("%{escaped}%"));
        qb.push(" ESCAPE '\\'");
    }
    if let Some(ref item_type) = filter.item_type {
        qb.push(" AND item_type = ");
        qb.push_bind(item_type.as_str());
    }
    if let Some(ref release) = filter.release {
        qb.push(
            " AND EXISTS (SELECT 1 FROM events e WHERE e.fingerprint = issues.fingerprint AND e.project_id = issues.project_id AND e.release = ",
        );
        qb.push_bind(release.as_str());
        qb.push(")");
    }
    if let Some(ts) = since {
        qb.push(" AND last_seen >= ");
        qb.push_bind(ts);
    }

    let rows = qb.build().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>(0))
        .collect())
}

/// Delete events by explicit IDs or by filter. Returns how many were removed.
pub async fn bulk_delete_events(
    pool: &DbPool,
    ids: Option<&[String]>,
    filter: Option<&EventFilter>,
    project_id: Option<u64>,
) -> Result<u64> {
    if let Some(ids) = ids {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut tx = pool.begin().await?;

        // Collect distinct fingerprints that will be affected before deleting.
        let mut affected_fps: Vec<String> = Vec::new();
        for chunk in ids.chunks(500) {
            let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
                "SELECT DISTINCT fingerprint FROM events WHERE fingerprint IS NOT NULL AND event_id IN (",
            );
            let mut sep = qb.separated(", ");
            for id in chunk {
                sep.push_bind(id.as_str());
            }
            qb.push(")");
            let rows = qb.build().fetch_all(&mut *tx).await?;
            affected_fps.extend(rows.into_iter().map(|row| row.get::<String, _>(0)));
        }

        let mut qb =
            sqlx::QueryBuilder::<crate::db::Db>::new("DELETE FROM events WHERE event_id IN (");
        let mut sep = qb.separated(", ");
        for id in ids {
            sep.push_bind(id.as_str());
        }
        qb.push(")");
        let deleted = qb.build().execute(&mut *tx).await?.rows_affected();
        tx.commit().await?;

        if deleted > 0 {
            super::retention::reconcile_after_event_delete(pool, &affected_fps).await?;
        }

        Ok(deleted)
    } else if let Some(filter) = filter {
        let mut f = EventFilter {
            level: filter.level.clone(),
            project_id: filter.project_id.or(project_id),
            query: filter.query.clone(),
            sort: None,
            item_type: filter.item_type.clone(),
        };
        if f.project_id.is_none() {
            f.project_id = project_id;
        }

        let mut tx = pool.begin().await?;

        // Build filter conditions once, apply to both SELECT and DELETE.
        macro_rules! push_filter {
            ($qb:ident, $f:ident) => {{
                #[allow(unused_assignments)]
                let mut first = true;
                macro_rules! push_sep {
                    () => {
                        if first {
                            $qb.push(" WHERE ");
                            #[allow(unused_assignments)]
                            {
                                first = false;
                            }
                        } else {
                            $qb.push(" AND ");
                        }
                    };
                }
                if let Some(ref level) = $f.level {
                    push_sep!();
                    $qb.push("level = ");
                    $qb.push_bind(level.clone());
                }
                if let Some(pid) = $f.project_id {
                    push_sep!();
                    $qb.push("project_id = ");
                    $qb.push_bind(pid as i64);
                }
                if let Some(ref query) = $f.query {
                    let escaped = query
                        .replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_");
                    push_sep!();
                    $qb.push("title LIKE ");
                    $qb.push_bind(format!("%{escaped}%"));
                    $qb.push(" ESCAPE '\\'");
                }
                if let Some(ref item_type) = $f.item_type {
                    push_sep!();
                    $qb.push("item_type = ");
                    $qb.push_bind(item_type.clone());
                }
            }};
        }

        // Check if filters produce any conditions (refuse to delete everything).
        if f.level.is_none() && f.project_id.is_none() && f.query.is_none() && f.item_type.is_none()
        {
            return Ok(0);
        }

        // Collect distinct fingerprints that will be affected before deleting.
        let mut sel_qb =
            sqlx::QueryBuilder::<crate::db::Db>::new("SELECT DISTINCT fingerprint FROM events");
        push_filter!(sel_qb, f);
        let affected_fps: Vec<String> = sel_qb
            .build()
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .filter_map(|row| row.get::<Option<String>, _>(0))
            .collect();

        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new("DELETE FROM events");
        push_filter!(qb, f);

        let deleted = qb.build().execute(&mut *tx).await?.rows_affected();
        tx.commit().await?;

        if deleted > 0 {
            super::retention::reconcile_after_event_delete(pool, &affected_fps).await?;
        }

        Ok(deleted)
    } else {
        Ok(0)
    }
}

/// Delete issues and all their events -- by fingerprints or by filter.
pub async fn bulk_delete_issues(
    pool: &DbPool,
    fingerprints: Option<&[String]>,
    filter: Option<&IssueFilter>,
    project_id: u64,
    since: Option<i64>,
) -> Result<u64> {
    let fps: Vec<String> = if let Some(fps) = fingerprints {
        fps.to_vec()
    } else if let Some(filter) = filter {
        fingerprints_by_filter(pool, project_id, filter, since).await?
    } else {
        return Ok(0);
    };

    if fps.is_empty() {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;

    for chunk in fps.chunks(500) {
        let mut qb =
            sqlx::QueryBuilder::<crate::db::Db>::new("DELETE FROM events WHERE fingerprint IN (");
        let mut sep = qb.separated(", ");
        for fp in chunk {
            sep.push_bind(fp.as_str());
        }
        qb.push(")");
        qb.build().execute(&mut *tx).await?;
    }

    for chunk in fps.chunks(500) {
        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
            "DELETE FROM issue_tag_values WHERE fingerprint IN (",
        );
        let mut sep = qb.separated(", ");
        for fp in chunk {
            sep.push_bind(fp.as_str());
        }
        qb.push(")");
        qb.build().execute(&mut *tx).await?;
    }

    let mut deleted: u64 = 0;
    for chunk in fps.chunks(500) {
        let mut qb =
            sqlx::QueryBuilder::<crate::db::Db>::new("DELETE FROM issues WHERE fingerprint IN (");
        let mut sep = qb.separated(", ");
        for fp in chunk {
            sep.push_bind(fp.as_str());
        }
        qb.push(")");
        deleted += qb.build().execute(&mut *tx).await?.rows_affected();
    }

    if deleted > 0 {
        #[cfg(feature = "sqlite")]
        sqlx::query("PRAGMA incremental_vacuum")
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(deleted)
}

/// Bulk status update -- by fingerprints or by filter.
pub async fn bulk_update_issue_status(
    pool: &DbPool,
    fingerprints: Option<&[String]>,
    filter: Option<&IssueFilter>,
    project_id: u64,
    since: Option<i64>,
    status: IssueStatus,
) -> Result<u64> {
    let fps: Vec<String> = if let Some(fps) = fingerprints {
        fps.to_vec()
    } else if let Some(filter) = filter {
        fingerprints_by_filter(pool, project_id, filter, since).await?
    } else {
        return Ok(0);
    };

    if fps.is_empty() {
        return Ok(0);
    }

    let mut updated: u64 = 0;
    for chunk in fps.chunks(500) {
        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new("UPDATE issues SET status = ");
        qb.push_bind(status.as_str());
        qb.push(" WHERE fingerprint IN (");
        let mut sep = qb.separated(", ");
        for fp in chunk {
            sep.push_bind(fp.as_str());
        }
        qb.push(")");
        updated += qb.build().execute(pool).await?.rows_affected();
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::test_helpers::*;
    use sqlx::Row;

    #[tokio::test]
    async fn bulk_delete_events_by_ids() {
        let pool = open_test_db().await;
        insert_test_event(&pool, "e1", 1, 100, Some("fp1"), Some("error"), Some("A")).await;
        insert_test_event(&pool, "e2", 1, 200, Some("fp1"), Some("error"), Some("B")).await;
        insert_test_event(&pool, "e3", 1, 300, Some("fp1"), Some("error"), Some("C")).await;

        let deleted = bulk_delete_events(
            &pool,
            Some(&["e1".to_string(), "e2".to_string()]),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(deleted, 2);

        let remaining: i64 = sqlx::query("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(remaining, 1);
    }

    #[tokio::test]
    async fn bulk_delete_events_by_filter() {
        let pool = open_test_db().await;
        insert_test_event(&pool, "e1", 1, 100, Some("fp1"), Some("error"), Some("A")).await;
        insert_test_event(&pool, "e2", 1, 200, Some("fp1"), Some("warning"), Some("B")).await;
        insert_test_event(&pool, "e3", 2, 300, Some("fp2"), Some("error"), Some("C")).await;

        let filter = EventFilter {
            level: Some("error".to_string()),
            project_id: Some(1),
            ..Default::default()
        };
        let deleted = bulk_delete_events(&pool, None, Some(&filter), None)
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        let remaining: i64 = sqlx::query("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(remaining, 2);
    }

    #[tokio::test]
    async fn bulk_delete_issues_by_fingerprints() {
        let pool = open_test_db().await;
        insert_test_event(&pool, "e1", 1, 100, Some("fp1"), Some("error"), Some("A")).await;
        insert_test_event(&pool, "e2", 1, 200, Some("fp2"), Some("error"), Some("B")).await;
        insert_test_issue(
            &pool,
            "fp1",
            1,
            Some("A"),
            Some("error"),
            100,
            100,
            1,
            "unresolved",
        )
        .await;
        insert_test_issue(
            &pool,
            "fp2",
            1,
            Some("B"),
            Some("error"),
            200,
            200,
            1,
            "unresolved",
        )
        .await;

        let deleted = bulk_delete_issues(&pool, Some(&["fp1".to_string()]), None, 1, None)
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        let issue_count: i64 = sqlx::query("SELECT COUNT(*) FROM issues")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(issue_count, 1);

        // Events for fp1 should be gone too
        let event_count: i64 = sqlx::query("SELECT COUNT(*) FROM events WHERE fingerprint = 'fp1'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(event_count, 0);
    }

    #[tokio::test]
    async fn bulk_update_issue_status_by_fingerprints() {
        let pool = open_test_db().await;
        insert_test_issue(
            &pool,
            "fp1",
            1,
            Some("A"),
            Some("error"),
            100,
            100,
            1,
            "unresolved",
        )
        .await;
        insert_test_issue(
            &pool,
            "fp2",
            1,
            Some("B"),
            Some("error"),
            200,
            200,
            1,
            "unresolved",
        )
        .await;

        let updated = bulk_update_issue_status(
            &pool,
            Some(&["fp1".to_string(), "fp2".to_string()]),
            None,
            1,
            None,
            IssueStatus::Resolved,
        )
        .await
        .unwrap();
        assert_eq!(updated, 2);

        let status: String = sqlx::query("SELECT status FROM issues WHERE fingerprint = 'fp1'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(status, "resolved");
    }

    #[tokio::test]
    async fn bulk_delete_empty_ids() {
        let pool = open_test_db().await;
        let deleted = bulk_delete_events(&pool, Some(&[]), None, None)
            .await
            .unwrap();
        assert_eq!(deleted, 0);
    }
}
