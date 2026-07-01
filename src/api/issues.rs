use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::domain::IssueStatus;
use crate::extractors::ReadPool;
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::{IssueFilter, Pagination};
use crate::server::AppState;

use super::ApiError;

#[derive(Deserialize)]
pub struct ListParams {
    pub status: Option<String>,
    pub level: Option<String>,
    pub query: Option<String>,
    #[serde(flatten)]
    pub page: Pagination,
}

#[derive(Deserialize)]
pub struct UpdateBody {
    pub status: IssueStatus,
}

/// GET /api/v1/projects/{project_id}/issues/?status=&level=&query=&limit=&offset=
pub async fn list_for_project(
    active: ActiveOrg,
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> Result<impl IntoResponse, ApiError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| ApiError::not_found("not found"))?;
    let filter = IssueFilter {
        level: params.level,
        status: params.status,
        query: params.query,
        sort: None,
        item_type: None,
        release: None,
        tag: None,
    };
    let page = params.page.page();
    let issues = queries::issues::list_issues(&pool, project_id, &filter, &page, None)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(issues))
}

/// GET /api/v1/issues/{fingerprint}/
pub async fn get(
    active: ActiveOrg,
    ReadPool(pool): ReadPool,
    Path(fingerprint): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let pid = crate::queries::orgs::project_of_fingerprint(&pool, &fingerprint)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("issue not found"))?;
    crate::orgs::extractor::require_project_scope(&active, &pool, pid)
        .await
        .map_err(|_| ApiError::not_found("not found"))?;
    let issue = queries::issues::get_issue(&pool, &fingerprint)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("issue not found"))?;
    Ok(Json(issue))
}

/// PUT /api/v1/issues/{fingerprint}/ with body {"status": "resolved"|"unresolved"|"ignored"}
pub async fn update_status(
    active: ActiveOrg,
    State(state): State<AppState>,
    Path(fingerprint): Path<String>,
    Json(body): Json<UpdateBody>,
) -> Result<impl IntoResponse, ApiError> {
    let pid = crate::queries::orgs::project_of_fingerprint(&state.pool, &fingerprint)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("issue not found"))?;
    crate::orgs::extractor::require_project_scope(&active, &state.pool, pid)
        .await
        .map_err(|_| ApiError::not_found("not found"))?;
    crate::orgs::extractor::require_owner(&active)
        .map_err(|_| ApiError::new(StatusCode::FORBIDDEN, "forbidden"))?;
    let affected =
        queries::issues::update_issue_status(&state.writer_pool, &fingerprint, body.status)
            .await
            .map_err(ApiError::internal)?;
    if affected == 0 {
        return Err(ApiError::not_found("issue not found"));
    }
    let issue = queries::issues::get_issue(&state.pool, &fingerprint)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("issue not found"))?;
    Ok(Json(issue))
}

#[cfg(test)]
mod tests {
    use crate::db::sql;
    use crate::orgs::extractor::{require_owner, require_project_scope, ActiveOrg};
    use crate::orgs::Role;
    use crate::queries::orgs::project_of_fingerprint;
    use crate::queries::test_helpers::insert_test_issue;
    use sqlx::Row;

    async fn insert_org(pool: &crate::db::DbPool, slug: &str) -> i64 {
        sqlx::query(sql!("INSERT INTO organizations (slug, name) VALUES (?1, 'T')"))
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

    async fn insert_project(pool: &crate::db::DbPool, project_id: i64, org_id: i64) {
        sqlx::query(sql!("INSERT INTO projects (project_id, org_id) VALUES (?1, ?2)"))
            .bind(project_id)
            .bind(org_id)
            .execute(pool)
            .await
            .unwrap();
    }

    // Member is blocked by require_owner (update_status gate).
    #[test]
    fn update_status_member_blocked_by_require_owner() {
        let member = ActiveOrg { org_id: 1, role: Some(Role::Member) };
        assert!(require_owner(&member).is_err());
    }

    // Owner passes require_owner.
    #[test]
    fn update_status_owner_allowed_by_require_owner() {
        let owner = ActiveOrg { org_id: 1, role: Some(Role::Owner) };
        assert!(require_owner(&owner).is_ok());
    }

    // Superuser passes require_owner.
    #[test]
    fn update_status_superuser_allowed_by_require_owner() {
        let su = ActiveOrg { org_id: 1, role: None };
        assert!(require_owner(&su).is_ok());
    }

    // Guard chain: resolve fingerprint -> scope check denies foreign-org caller.
    #[tokio::test]
    async fn issues_get_guard_denies_foreign_org_fingerprint() {
        let pool = crate::db::open_test_pool().await;
        let org_a = insert_org(&pool, "iss-guard-a").await;
        let org_b = insert_org(&pool, "iss-guard-b").await;
        insert_project(&pool, 6001, org_a).await;
        insert_test_issue(&pool, "iss-fp-guard", 6001, None, None, 0, 0, 0, "unresolved").await;

        let pid = project_of_fingerprint(&pool, "iss-fp-guard")
            .await
            .unwrap()
            .unwrap();

        let owner_a = ActiveOrg { org_id: org_a, role: Some(Role::Owner) };
        let owner_b = ActiveOrg { org_id: org_b, role: Some(Role::Owner) };

        assert!(require_project_scope(&owner_a, &pool, pid).await.is_ok());
        assert!(require_project_scope(&owner_b, &pool, pid).await.is_err());
    }

    // Full update_status guard sequence: member of the correct org is blocked.
    #[tokio::test]
    async fn update_status_member_in_own_org_blocked() {
        let pool = crate::db::open_test_pool().await;
        let org = insert_org(&pool, "iss-upd-org").await;
        insert_project(&pool, 6002, org).await;
        insert_test_issue(&pool, "iss-fp-upd", 6002, None, None, 0, 0, 0, "unresolved").await;

        let pid = project_of_fingerprint(&pool, "iss-fp-upd")
            .await
            .unwrap()
            .unwrap();

        let member = ActiveOrg { org_id: org, role: Some(Role::Member) };

        // Scope check passes (correct org).
        assert!(require_project_scope(&member, &pool, pid).await.is_ok());
        // Owner check blocks.
        assert!(require_owner(&member).is_err());
    }
}
