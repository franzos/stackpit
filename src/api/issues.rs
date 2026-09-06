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
    pub environment: Option<String>,
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
        environment: params.environment,
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
    let (pid, _) = super::resolve_issue_project(&active, &pool, &fingerprint).await?;
    let issue = queries::issues::get_issue(&pool, pid, &fingerprint)
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
    let (pid, scope) = super::resolve_issue_project(&active, &state.pool, &fingerprint).await?;
    crate::orgs::extractor::require_owner(&scope)
        .map_err(|_| ApiError::new(StatusCode::FORBIDDEN, "forbidden"))?;
    let affected =
        queries::issues::update_issue_status(&state.writer_pool, pid, &fingerprint, body.status)
            .await
            .map_err(ApiError::internal)?;
    if affected == 0 {
        return Err(ApiError::not_found("issue not found"));
    }
    let issue = queries::issues::get_issue(&state.pool, pid, &fingerprint)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("issue not found"))?;
    Ok(Json(issue))
}

#[cfg(test)]
mod tests {
    use crate::api::resolve_issue_project;
    use crate::db::sql;
    use crate::orgs::extractor::{require_owner, require_project_scope, ActiveOrg, ProjectScope};
    use crate::orgs::Role;
    use crate::queries::test_helpers::insert_test_issue;
    use sqlx::Row;

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

    fn scope(role: Option<Role>) -> ProjectScope {
        ProjectScope { org_id: 1, role }
    }

    // Member is blocked by require_owner (update_status gate).
    #[test]
    fn update_status_member_blocked_by_require_owner() {
        assert!(require_owner(&scope(Some(Role::Member))).is_err());
    }

    // Owner passes require_owner.
    #[test]
    fn update_status_owner_allowed_by_require_owner() {
        assert!(require_owner(&scope(Some(Role::Owner))).is_ok());
    }

    // Superuser passes require_owner.
    #[test]
    fn update_status_superuser_allowed_by_require_owner() {
        assert!(require_owner(&scope(None)).is_ok());
    }

    // Guard chain: resolve fingerprint -> scope check denies foreign-org caller.
    #[tokio::test]
    async fn issues_get_guard_denies_foreign_org_fingerprint() {
        let pool = crate::db::open_test_pool().await;
        let org_a = insert_org(&pool, "iss-guard-a").await;
        let org_b = insert_org(&pool, "iss-guard-b").await;
        insert_project(&pool, 6001, org_a).await;
        insert_test_issue(
            &pool,
            "iss-fp-guard",
            6001,
            None,
            None,
            0,
            0,
            0,
            "unresolved",
        )
        .await;

        let owner_a =
            ActiveOrg::with_memberships(org_a, Some(Role::Owner), vec![(org_a, Role::Owner)]);
        let owner_b =
            ActiveOrg::with_memberships(org_b, Some(Role::Owner), vec![(org_b, Role::Owner)]);

        let (pid, _) = resolve_issue_project(&owner_a, &pool, "iss-fp-guard")
            .await
            .ok()
            .expect("owner of the org resolves the issue");
        assert_eq!(pid, 6001);
        assert!(require_project_scope(&owner_b, &pool, pid as i64)
            .await
            .is_err());
        assert!(resolve_issue_project(&owner_b, &pool, "iss-fp-guard")
            .await
            .is_err());
    }

    // The same fingerprint in two projects: a member of one org sees exactly
    // that one, a member of neither gets 404, and a superuser who can see both
    // gets 404 rather than a guess.
    #[tokio::test]
    async fn resolve_issue_project_handles_a_fingerprint_shared_across_projects() {
        let pool = crate::db::open_test_pool().await;
        let org_a = insert_org(&pool, "iss-amb-a").await;
        let org_b = insert_org(&pool, "iss-amb-b").await;
        let org_c = insert_org(&pool, "iss-amb-c").await;
        insert_project(&pool, 6011, org_a).await;
        insert_project(&pool, 6012, org_b).await;
        insert_test_issue(&pool, "iss-fp-amb", 6011, None, None, 0, 0, 0, "unresolved").await;
        insert_test_issue(&pool, "iss-fp-amb", 6012, None, None, 0, 0, 0, "unresolved").await;

        let member_a =
            ActiveOrg::with_memberships(org_a, Some(Role::Member), vec![(org_a, Role::Member)]);
        let (pid, _) = resolve_issue_project(&member_a, &pool, "iss-fp-amb")
            .await
            .ok()
            .expect("member of org A resolves to A's project");
        assert_eq!(pid, 6011);

        let member_c =
            ActiveOrg::with_memberships(org_c, Some(Role::Member), vec![(org_c, Role::Member)]);
        assert!(resolve_issue_project(&member_c, &pool, "iss-fp-amb")
            .await
            .is_err());

        let superuser = ActiveOrg::bare(999, None);
        assert!(
            resolve_issue_project(&superuser, &pool, "iss-fp-amb")
                .await
                .is_err(),
            "two visible candidates is ambiguous, not a guess"
        );
        assert!(resolve_issue_project(&superuser, &pool, "iss-fp-none")
            .await
            .is_err());
    }

    // Full update_status guard sequence: member of the correct org is blocked.
    #[tokio::test]
    async fn update_status_member_in_own_org_blocked() {
        let pool = crate::db::open_test_pool().await;
        let org = insert_org(&pool, "iss-upd-org").await;
        insert_project(&pool, 6002, org).await;
        insert_test_issue(&pool, "iss-fp-upd", 6002, None, None, 0, 0, 0, "unresolved").await;

        let member =
            ActiveOrg::with_memberships(org, Some(Role::Member), vec![(org, Role::Member)]);

        // Scope check passes (correct org), owner check blocks.
        let (pid, scope) = resolve_issue_project(&member, &pool, "iss-fp-upd")
            .await
            .ok()
            .expect("member resolves the issue");
        assert_eq!(pid, 6002);
        assert!(require_owner(&scope).is_err());
    }
}
