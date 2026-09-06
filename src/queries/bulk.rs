use anyhow::Result;
use sqlx::Row;

use crate::domain::IssueStatus;

use super::types::{EventFilter, IssueFilter};

use crate::db::DbPool;

/// Per-chunk row cap; keeps each delete transaction short so the ingest writer can interleave.
const CHUNK_LIMIT: i64 = 5000;

/// Pause between chunks so a waiting writer can grab the DB write lock.
const CHUNK_PAUSE: std::time::Duration = std::time::Duration::from_millis(100);

/// Resolve a filter down to a list of fingerprints.
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
        qb.push(" AND title LIKE ");
        qb.push_bind(super::like_contains(query));
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

/// Returns the subset of `fingerprints` that belong to `project_id`.
/// Prevents callers from supplying foreign fingerprints to bypass project scope.
async fn filter_fingerprints_to_project(
    pool: &DbPool,
    fingerprints: &[String],
    project_id: u64,
) -> Result<Vec<String>> {
    if fingerprints.is_empty() {
        return Ok(vec![]);
    }
    let mut fps = Vec::new();
    for chunk in fingerprints.chunks(500) {
        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
            "SELECT fingerprint FROM issues WHERE project_id = ",
        );
        qb.push_bind(project_id as i64);
        qb.push(" AND fingerprint IN (");
        let mut sep = qb.separated(", ");
        for fp in chunk {
            sep.push_bind(fp.as_str());
        }
        qb.push(")");
        let rows = qb.build().fetch_all(pool).await?;
        fps.extend(rows.into_iter().map(|r| r.get::<String, _>(0)));
    }
    Ok(fps)
}

/// Delete events by IDs or filter; `org_id` constrains to org's projects (None = superuser only).
pub async fn bulk_delete_events(
    pool: &DbPool,
    ids: Option<&[String]>,
    filter: Option<&EventFilter>,
    project_id: Option<u64>,
    org_id: Option<i64>,
) -> Result<u64> {
    bulk_delete_events_chunked(pool, ids, filter, project_id, org_id, CHUNK_LIMIT).await
}

async fn bulk_delete_events_chunked(
    pool: &DbPool,
    ids: Option<&[String]>,
    filter: Option<&EventFilter>,
    project_id: Option<u64>,
    org_id: Option<i64>,
    chunk_limit: i64,
) -> Result<u64> {
    // rowid (sqlite) / ctid (postgres) give a stable per-chunk ordering; see retention.rs.
    #[cfg(feature = "sqlite")]
    let rid = "rowid";
    #[cfg(not(feature = "sqlite"))]
    let rid = "ctid";

    if let Some(ids) = ids {
        if ids.is_empty() {
            return Ok(0);
        }

        // Collect distinct issue keys that will be affected before deleting.
        let mut affected_fps: Vec<(i64, String)> = Vec::new();
        for chunk in ids.chunks(500) {
            let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
                "SELECT DISTINCT project_id, fingerprint FROM events WHERE fingerprint IS NOT NULL AND event_id IN (",
            );
            let mut sep = qb.separated(", ");
            for id in chunk {
                sep.push_bind(id.as_str());
            }
            qb.push(")");
            if let Some(oid) = org_id {
                qb.push(" AND project_id IN (SELECT project_id FROM projects WHERE org_id = ");
                qb.push_bind(oid);
                qb.push(")");
            } else if let Some(pid) = project_id {
                qb.push(" AND project_id = ");
                qb.push_bind(pid as i64);
            }
            let rows = qb.build().fetch_all(pool).await?;
            affected_fps.extend(
                rows.into_iter()
                    .map(|row| (row.get::<i64, _>(0), row.get::<String, _>(1))),
            );
        }

        // Delete one id-chunk per short transaction so the writer actor can interleave.
        let id_chunks: Vec<&[String]> = ids.chunks(500).collect();
        let mut deleted: u64 = 0;
        for (i, chunk) in id_chunks.iter().enumerate() {
            let mut tx = pool.begin().await?;
            let mut qb =
                sqlx::QueryBuilder::<crate::db::Db>::new("DELETE FROM events WHERE event_id IN (");
            let mut sep = qb.separated(", ");
            for id in *chunk {
                sep.push_bind(id.as_str());
            }
            qb.push(")");
            if let Some(oid) = org_id {
                qb.push(" AND project_id IN (SELECT project_id FROM projects WHERE org_id = ");
                qb.push_bind(oid);
                qb.push(")");
            } else if let Some(pid) = project_id {
                qb.push(" AND project_id = ");
                qb.push_bind(pid as i64);
            }
            deleted += qb.build().execute(&mut *tx).await?.rows_affected();
            tx.commit().await?;

            if i + 1 < id_chunks.len() {
                tokio::time::sleep(CHUNK_PAUSE).await;
            }
        }

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
                    push_sep!();
                    $qb.push("title LIKE ");
                    $qb.push_bind(super::like_contains(query));
                    $qb.push(" ESCAPE '\\'");
                }
                if let Some(ref item_type) = $f.item_type {
                    push_sep!();
                    $qb.push("item_type = ");
                    $qb.push_bind(item_type.clone());
                }
            }};
        }

        // Refuse to delete everything when no constraints apply at all.
        if f.level.is_none()
            && f.project_id.is_none()
            && f.query.is_none()
            && f.item_type.is_none()
            && org_id.is_none()
        {
            return Ok(0);
        }

        // Tracks whether push_filter emitted WHERE/AND, to pick the right connector for org constraint.
        let has_field_filter = f.level.is_some()
            || f.project_id.is_some()
            || f.query.is_some()
            || f.item_type.is_some();

        // Collect distinct issue keys that will be affected before deleting.
        let mut sel_qb = sqlx::QueryBuilder::<crate::db::Db>::new(
            "SELECT DISTINCT project_id, fingerprint FROM events",
        );
        push_filter!(sel_qb, f);
        if let Some(oid) = org_id {
            if f.project_id.is_none() {
                if has_field_filter {
                    sel_qb.push(" AND ");
                } else {
                    sel_qb.push(" WHERE ");
                }
                sel_qb.push("project_id IN (SELECT project_id FROM projects WHERE org_id = ");
                sel_qb.push_bind(oid);
                sel_qb.push(")");
            }
        }
        let affected_fps: Vec<(i64, String)> = sel_qb
            .build()
            .fetch_all(pool)
            .await?
            .into_iter()
            .filter_map(|row| {
                row.get::<Option<String>, _>(1)
                    .map(|fp| (row.get::<i64, _>(0), fp))
            })
            .collect();

        // Chunked delete: each chunk is its own short transaction so the writer actor can interleave flushes, with scoping entirely in the inner SELECT so the outer DELETE only touches in-scope rows.
        let mut deleted: u64 = 0;
        loop {
            let mut tx = pool.begin().await?;
            let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(format!(
                "DELETE FROM events WHERE {rid} IN (SELECT {rid} FROM events"
            ));
            push_filter!(qb, f);
            if let Some(oid) = org_id {
                if f.project_id.is_none() {
                    if has_field_filter {
                        qb.push(" AND ");
                    } else {
                        qb.push(" WHERE ");
                    }
                    qb.push("project_id IN (SELECT project_id FROM projects WHERE org_id = ");
                    qb.push_bind(oid);
                    qb.push(")");
                }
            }
            qb.push(format!(" ORDER BY {rid} LIMIT "));
            qb.push_bind(chunk_limit);
            qb.push(")");

            let n = qb.build().execute(&mut *tx).await?.rows_affected();
            tx.commit().await?;
            deleted += n;

            if n < chunk_limit as u64 {
                break;
            }
            tokio::time::sleep(CHUNK_PAUSE).await;
        }

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
    bulk_delete_issues_chunked(pool, fingerprints, filter, project_id, since, CHUNK_LIMIT).await
}

async fn bulk_delete_issues_chunked(
    pool: &DbPool,
    fingerprints: Option<&[String]>,
    filter: Option<&IssueFilter>,
    project_id: u64,
    since: Option<i64>,
    chunk_limit: i64,
) -> Result<u64> {
    let fps: Vec<String> = if let Some(fps) = fingerprints {
        // Constrain to project_id before acting; prevents cross-project id injection.
        filter_fingerprints_to_project(pool, fps, project_id).await?
    } else if let Some(filter) = filter {
        fingerprints_by_filter(pool, project_id, filter, since).await?
    } else {
        return Ok(0);
    };

    if fps.is_empty() {
        return Ok(0);
    }

    // rowid (sqlite) / ctid (postgres) give a stable per-chunk ordering; see retention.rs.
    #[cfg(feature = "sqlite")]
    let rid = "rowid";
    #[cfg(not(feature = "sqlite"))]
    let rid = "ctid";

    // Event deletion is the unbounded part (one fingerprint can own hundreds of thousands of events), so delete in row-chunks, each its own short transaction, with project_id + fingerprint scoping inside every inner SELECT.
    for chunk in fps.chunks(500) {
        loop {
            let mut tx = pool.begin().await?;
            let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(format!(
                "DELETE FROM events WHERE {rid} IN (SELECT {rid} FROM events WHERE project_id = "
            ));
            qb.push_bind(project_id as i64);
            qb.push(" AND fingerprint IN (");
            let mut sep = qb.separated(", ");
            for fp in chunk {
                sep.push_bind(fp.as_str());
            }
            qb.push(")");
            qb.push(format!(" ORDER BY {rid} LIMIT "));
            qb.push_bind(chunk_limit);
            qb.push(")");

            let n = qb.build().execute(&mut *tx).await?.rows_affected();
            tx.commit().await?;

            if n < chunk_limit as u64 {
                break;
            }
            tokio::time::sleep(CHUNK_PAUSE).await;
        }
    }

    // Metadata cleanup is bounded by the fingerprint (issue) count, so it stays a single final transaction.
    let mut tx = pool.begin().await?;

    for chunk in fps.chunks(500) {
        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
            "DELETE FROM issue_tag_values WHERE project_id = ",
        );
        qb.push_bind(project_id as i64);
        qb.push(" AND fingerprint IN (");
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
            sqlx::QueryBuilder::<crate::db::Db>::new("DELETE FROM issues WHERE project_id = ");
        qb.push_bind(project_id as i64);
        qb.push(" AND fingerprint IN (");
        let mut sep = qb.separated(", ");
        for fp in chunk {
            sep.push_bind(fp.as_str());
        }
        qb.push(")");
        deleted += qb.build().execute(&mut *tx).await?.rows_affected();
    }

    tx.commit().await?;

    #[cfg(feature = "sqlite")]
    if deleted > 0 {
        if let Err(e) = super::retention::incremental_vacuum(pool).await {
            tracing::warn!("bulk_delete_issues: incremental_vacuum failed: {e}");
        }
    }

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
        qb.push(" WHERE project_id = ");
        qb.push_bind(project_id as i64);
        qb.push(" AND fingerprint IN (");
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
    use crate::db::sql;
    use crate::queries::test_helpers::*;
    use sqlx::Row;

    async fn insert_test_org(pool: &DbPool, slug: &str) -> i64 {
        sqlx::query(sql!(
            "INSERT INTO organizations (slug, name) VALUES (?1, ?2)"
        ))
        .bind(slug)
        .bind(slug)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap()
            .get(0)
    }

    async fn insert_test_project(pool: &DbPool, project_id: i64, org_id: i64) {
        sqlx::query(sql!(
            "INSERT INTO projects (project_id, org_id) VALUES (?1, ?2)"
        ))
        .bind(project_id)
        .bind(org_id)
        .execute(pool)
        .await
        .unwrap();
    }

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
        let deleted = bulk_delete_events(&pool, None, Some(&filter), None, None)
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

    // A by-filter delete that spans more than one chunk removes every targeted row, leaves other projects untouched, and reconciles the emptied issue.
    #[tokio::test]
    async fn bulk_delete_events_filter_spans_multiple_chunks() {
        let pool = open_test_db().await;
        for i in 0..5 {
            insert_test_event(
                &pool,
                &format!("m{i}"),
                1,
                100 + i,
                Some("fp1"),
                Some("error"),
                Some("A"),
            )
            .await;
        }
        insert_test_event(&pool, "keep", 2, 100, Some("fp2"), Some("error"), Some("B")).await;
        insert_test_issue(
            &pool,
            "fp1",
            1,
            Some("A"),
            Some("error"),
            100,
            104,
            5,
            "unresolved",
        )
        .await;
        insert_test_issue(
            &pool,
            "fp2",
            2,
            Some("B"),
            Some("error"),
            100,
            100,
            1,
            "unresolved",
        )
        .await;

        let filter = EventFilter {
            level: Some("error".to_string()),
            project_id: Some(1),
            ..Default::default()
        };
        // chunk_limit 2 over 5 rows forces three chunks.
        let deleted = bulk_delete_events_chunked(&pool, None, Some(&filter), None, None, 2)
            .await
            .unwrap();
        assert_eq!(deleted, 5);

        let p1: i64 = sqlx::query("SELECT COUNT(*) FROM events WHERE project_id = 1")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(p1, 0, "all targeted rows gone");

        let p2: i64 = sqlx::query("SELECT COUNT(*) FROM events WHERE project_id = 2")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(p2, 1, "other project untouched");

        // fp1 emptied -> reconciled away; fp2 untouched.
        let fp1: Option<i64> =
            sqlx::query_scalar("SELECT event_count FROM issues WHERE fingerprint = 'fp1'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(fp1, None, "emptied issue reconciled away");
        let fp2: Option<i64> =
            sqlx::query_scalar("SELECT event_count FROM issues WHERE fingerprint = 'fp2'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(fp2, Some(1), "other project's issue survives");
    }

    // Deleting an issue whose events span more than one chunk removes all its events and the issue, leaving other projects untouched.
    #[tokio::test]
    async fn bulk_delete_issues_events_span_multiple_chunks() {
        let pool = open_test_db().await;
        for i in 0..5 {
            insert_test_event(
                &pool,
                &format!("i{i}"),
                1,
                100 + i,
                Some("fp1"),
                Some("error"),
                Some("A"),
            )
            .await;
        }
        insert_test_event(&pool, "keep", 2, 100, Some("fp2"), Some("error"), Some("B")).await;
        insert_test_issue(
            &pool,
            "fp1",
            1,
            Some("A"),
            Some("error"),
            100,
            104,
            5,
            "unresolved",
        )
        .await;
        insert_test_issue(
            &pool,
            "fp2",
            2,
            Some("B"),
            Some("error"),
            100,
            100,
            1,
            "unresolved",
        )
        .await;

        // chunk_limit 2 over 5 events forces three chunks.
        let deleted =
            bulk_delete_issues_chunked(&pool, Some(&["fp1".to_string()]), None, 1, None, 2)
                .await
                .unwrap();
        assert_eq!(deleted, 1, "one issue deleted");

        let p1: i64 = sqlx::query("SELECT COUNT(*) FROM events WHERE project_id = 1")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(p1, 0, "all fp1 events gone");

        let remaining: i64 = sqlx::query("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(remaining, 1, "other project's event survives");

        let issues: i64 = sqlx::query("SELECT COUNT(*) FROM issues WHERE fingerprint = 'fp2'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(issues, 1, "other project's issue survives");
    }

    #[tokio::test]
    async fn bulk_delete_empty_ids() {
        let pool = open_test_db().await;
        let deleted = bulk_delete_events(&pool, Some(&[]), None, None, None)
            .await
            .unwrap();
        assert_eq!(deleted, 0);
    }

    // Security: supplied event_ids must not reach across orgs.
    #[tokio::test]
    async fn bulk_delete_events_org_scope_ids_cannot_cross_org() {
        let pool = open_test_db().await;
        let org_a = insert_test_org(&pool, "bulk-sec-org-a").await;
        let org_b = insert_test_org(&pool, "bulk-sec-org-b").await;
        insert_test_project(&pool, 901, org_a).await;
        insert_test_project(&pool, 902, org_b).await;

        // e1 belongs to org_a/project 901; e2 belongs to org_b/project 902.
        insert_test_event(&pool, "sec-e1", 901, 100, None, Some("error"), Some("A")).await;
        insert_test_event(&pool, "sec-e2", 902, 200, None, Some("error"), Some("B")).await;

        // Scoped to org_a but supplying org_b's event id.
        let deleted = bulk_delete_events(
            &pool,
            Some(&["sec-e2".to_string()]),
            None,
            None,
            Some(org_a),
        )
        .await
        .unwrap();
        assert_eq!(deleted, 0, "cross-org id must not delete");

        let count: i64 = sqlx::query("SELECT COUNT(*) FROM events WHERE event_id = 'sec-e2'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(count, 1, "org_b event must survive");
    }

    // Security: filter-path must not reach across orgs.
    #[tokio::test]
    async fn bulk_delete_events_org_scope_filter_cannot_cross_org() {
        let pool = open_test_db().await;
        let org_a = insert_test_org(&pool, "bulk-sec-filter-org-a").await;
        let org_b = insert_test_org(&pool, "bulk-sec-filter-org-b").await;
        insert_test_project(&pool, 911, org_a).await;
        insert_test_project(&pool, 912, org_b).await;

        insert_test_event(&pool, "sf-e1", 911, 100, None, Some("error"), Some("A")).await;
        insert_test_event(&pool, "sf-e2", 912, 200, None, Some("error"), Some("B")).await;

        // All-matching delete scoped to org_a; org_b event must survive.
        let filter = EventFilter {
            level: Some("error".to_string()),
            ..Default::default()
        };
        let deleted = bulk_delete_events(&pool, None, Some(&filter), None, Some(org_a))
            .await
            .unwrap();
        assert_eq!(deleted, 1, "only org_a event deleted");

        let count: i64 = sqlx::query("SELECT COUNT(*) FROM events WHERE event_id = 'sf-e2'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(count, 1, "org_b event must survive");
    }

    // Security: supplied fingerprints must not reach across projects.
    #[tokio::test]
    async fn bulk_delete_issues_project_scope_ids_cannot_cross_project() {
        let pool = open_test_db().await;

        // Project 801: own issue with fp "sec-fp-a".
        // Project 802: foreign issue with fp "sec-fp-b".
        insert_test_issue(
            &pool,
            "sec-fp-a",
            801,
            Some("A"),
            Some("error"),
            1,
            1,
            1,
            "unresolved",
        )
        .await;
        insert_test_issue(
            &pool,
            "sec-fp-b",
            802,
            Some("B"),
            Some("error"),
            1,
            1,
            1,
            "unresolved",
        )
        .await;

        // Try to delete project 802's fingerprint while scoped to project 801.
        let deleted = bulk_delete_issues(&pool, Some(&["sec-fp-b".to_string()]), None, 801, None)
            .await
            .unwrap();
        assert_eq!(deleted, 0, "cross-project fingerprint must not delete");

        let count: i64 = sqlx::query("SELECT COUNT(*) FROM issues WHERE fingerprint = 'sec-fp-b'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(count, 1, "project 802 issue must survive");
    }

    // Security: supplied fingerprints must not update status across projects.
    #[tokio::test]
    async fn bulk_update_issue_status_project_scope_ids_cannot_cross_project() {
        let pool = open_test_db().await;

        insert_test_issue(
            &pool,
            "upd-fp-b",
            802,
            Some("B"),
            Some("error"),
            1,
            1,
            1,
            "unresolved",
        )
        .await;

        // Attempt to resolve project 802's issue while scoped to project 801.
        let updated = bulk_update_issue_status(
            &pool,
            Some(&["upd-fp-b".to_string()]),
            None,
            801,
            None,
            IssueStatus::Resolved,
        )
        .await
        .unwrap();
        assert_eq!(
            updated, 0,
            "cross-project fingerprint must not update status"
        );

        let status: String =
            sqlx::query("SELECT status FROM issues WHERE fingerprint = 'upd-fp-b'")
                .fetch_one(&pool)
                .await
                .unwrap()
                .get(0);
        assert_eq!(
            status, "unresolved",
            "project 802 issue status must not change"
        );
    }
}
