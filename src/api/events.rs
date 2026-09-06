use axum::extract::{Path, Query};
use axum::response::IntoResponse;

use crate::extractors::ReadPool;
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::Pagination;

use super::ApiError;

/// GET /api/v1/projects/{project_id}/events/?limit=&offset=
pub async fn list_for_project(
    active: ActiveOrg,
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<Pagination>,
) -> Result<impl IntoResponse, ApiError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| ApiError::not_found("not found"))?;
    let page = params.page();
    let events = queries::events::list_events(&pool, project_id, &page)
        .await
        .map_err(ApiError::internal)?;
    Ok(axum::Json(events))
}

/// GET /api/v1/issues/{fingerprint}/events/?limit=&offset=
pub async fn list_for_issue(
    active: ActiveOrg,
    ReadPool(pool): ReadPool,
    Path(fingerprint): Path<String>,
    Query(params): Query<Pagination>,
) -> Result<impl IntoResponse, ApiError> {
    let (pid, _) = super::resolve_issue_project(&active, &pool, &fingerprint).await?;
    let page = params.page();
    let events = queries::events::list_events_for_issue(&pool, pid, &fingerprint, &page)
        .await
        .map_err(ApiError::internal)?;
    Ok(axum::Json(events))
}

/// GET /api/v1/issues/{fingerprint}/events/latest/
pub async fn latest_for_issue(
    active: ActiveOrg,
    ReadPool(pool): ReadPool,
    Path(fingerprint): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let (pid, _) = super::resolve_issue_project(&active, &pool, &fingerprint).await?;
    let event = queries::events::get_latest_event_for_issue(&pool, pid, &fingerprint)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("no events found for issue"))?;
    Ok(axum::Json(event))
}

/// GET /api/v1/events/{event_id}/
pub async fn get(
    active: ActiveOrg,
    ReadPool(pool): ReadPool,
    Path(event_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let pid = crate::queries::orgs::project_of_event(&pool, &event_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("event not found"))?;
    crate::orgs::extractor::require_project_scope(&active, &pool, pid)
        .await
        .map_err(|_| ApiError::not_found("not found"))?;
    let event = queries::events::get_event_detail(&pool, &event_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("event not found"))?;
    Ok(axum::Json(event))
}

#[cfg(test)]
mod tests {
    use crate::api::resolve_issue_project;
    use crate::db::sql;
    use crate::orgs::extractor::{require_project_scope, ActiveOrg};
    use crate::orgs::Role;
    use crate::queries::test_helpers::insert_test_issue;
    use sqlx::Row;

    async fn insert_project(pool: &crate::db::DbPool, project_id: i64, org_id: i64) {
        sqlx::query(sql!(
            "INSERT INTO projects (project_id, org_id) VALUES (?1, ?2)"
        ))
        .bind(project_id)
        .bind(org_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_org(pool: &crate::db::DbPool, slug: &str) -> i64 {
        sqlx::query(sql!(
            "INSERT INTO organizations (slug, name) VALUES (?1, 'T')"
        ))
        .bind(slug)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap()
            .get("org_id")
    }

    // Verify the guard chain: resolve fingerprint -> scope check denies foreign org.
    #[tokio::test]
    async fn events_list_for_issue_guard_denies_foreign_org() {
        let pool = crate::db::open_test_pool().await;
        let org_a = insert_org(&pool, "evts-org-a").await;
        let org_b = insert_org(&pool, "evts-org-b").await;
        insert_project(&pool, 5001, org_a).await;
        insert_test_issue(&pool, "evts-fp-1", 5001, None, None, 0, 0, 0, "unresolved").await;

        let member_a =
            ActiveOrg::with_memberships(org_a, Some(Role::Member), vec![(org_a, Role::Member)]);
        let member_b =
            ActiveOrg::with_memberships(org_b, Some(Role::Member), vec![(org_b, Role::Member)]);

        let (pid, _) = resolve_issue_project(&member_a, &pool, "evts-fp-1")
            .await
            .ok()
            .expect("member of org A resolves the issue");
        assert_eq!(pid, 5001);
        assert!(require_project_scope(&member_b, &pool, pid as i64)
            .await
            .is_err());
        assert!(resolve_issue_project(&member_b, &pool, "evts-fp-1")
            .await
            .is_err());
    }

    // Superuser skips scope check even for a fingerprint owned by a different org.
    #[tokio::test]
    async fn events_list_for_issue_guard_superuser_bypasses() {
        let pool = crate::db::open_test_pool().await;
        let org_a = insert_org(&pool, "evts-su-org").await;
        insert_project(&pool, 5002, org_a).await;
        insert_test_issue(&pool, "evts-fp-su", 5002, None, None, 0, 0, 0, "unresolved").await;

        let superuser = ActiveOrg::bare(999, None);
        let (pid, _) = resolve_issue_project(&superuser, &pool, "evts-fp-su")
            .await
            .ok()
            .expect("superuser resolves a single-project fingerprint");
        assert_eq!(pid, 5002);
        assert!(require_project_scope(&superuser, &pool, pid as i64)
            .await
            .is_ok());
    }

    // project_of_event returns None for an unknown event_id (handler maps to 404).
    #[tokio::test]
    async fn events_get_guard_missing_event_id_returns_none() {
        let pool = crate::db::open_test_pool().await;
        let result = crate::queries::orgs::project_of_event(&pool, "no-such-event")
            .await
            .unwrap();
        assert!(result.is_none());
    }
}
