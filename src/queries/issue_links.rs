use crate::db::{sql, DbPool};
use sqlx::Row;

pub struct ExternalLink {
    pub id: i64,
    /// `None` once the integration it was filed through is deleted; the link outlives it.
    pub integration_id: Option<i64>,
    /// Denormalised at file time so the row keeps meaning without its integration.
    pub integration_kind: String,
    pub integration_name: String,
    pub external_id: String,
    pub external_url: String,
    pub external_state: Option<String>,
}

/// Insert-first idempotency guard: returns true if THIS insert created the row,
/// false if a link for (fingerprint, integration_id) already existed.
#[allow(clippy::too_many_arguments)]
pub async fn insert_link(
    pool: &DbPool,
    project_id: i64,
    fingerprint: &str,
    integration_id: i64,
    integration_name: &str,
    integration_kind: &str,
    external_id: &str,
    external_url: &str,
    external_state: Option<&str>,
    created_at: i64,
) -> anyhow::Result<bool> {
    let res = sqlx::query(sql!(
        "INSERT INTO issue_external_links \
         (project_id, fingerprint, integration_id, integration_name, integration_kind, \
          external_id, external_url, external_state, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
         ON CONFLICT (fingerprint, integration_id) DO NOTHING"
    ))
    .bind(project_id)
    .bind(fingerprint)
    .bind(integration_id)
    .bind(integration_name)
    .bind(integration_kind)
    .bind(external_id)
    .bind(external_url)
    .bind(external_state)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() == 1)
}

/// Remove a link locally; the issue stays on the forge. Scoped by project.
pub async fn delete_link(pool: &DbPool, project_id: i64, link_id: i64) -> anyhow::Result<u64> {
    let res = sqlx::query(sql!(
        "DELETE FROM issue_external_links WHERE id = ?1 AND project_id = ?2"
    ))
    .bind(link_id)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// Links filed for one issue - scoped by project, since two projects can share a fingerprint.
pub async fn links_for_issue(
    pool: &DbPool,
    project_id: i64,
    fingerprint: &str,
) -> anyhow::Result<Vec<ExternalLink>> {
    // No join: a link must still render after its integration is deleted.
    let rows = sqlx::query(sql!(
        "SELECT id, integration_id, integration_kind, integration_name, \
                external_id, external_url, external_state \
         FROM issue_external_links WHERE fingerprint = ?1 AND project_id = ?2 \
         ORDER BY created_at, id"
    ))
    .bind(fingerprint)
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ExternalLink {
            id: r.get("id"),
            integration_id: r.get("integration_id"),
            integration_kind: r.get("integration_kind"),
            integration_name: r.get("integration_name"),
            external_id: r.get("external_id"),
            external_url: r.get("external_url"),
            external_state: r.get("external_state"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::integrations::create_integration;
    use crate::queries::test_helpers::open_test_db;

    async fn seed_github_integration(pool: &DbPool) -> i64 {
        create_integration(pool, 1, "gh-intg", "github", None, None, None, false, false)
            .await
            .unwrap();
        sqlx::query(sql!("SELECT id FROM integrations WHERE name = 'gh-intg'"))
            .fetch_one(pool)
            .await
            .unwrap()
            .get(0)
    }

    #[allow(clippy::too_many_arguments)]
    async fn link(
        pool: &DbPool,
        project_id: i64,
        fingerprint: &str,
        integration_id: i64,
        external_id: &str,
        external_url: &str,
        state: Option<&str>,
        at: i64,
    ) -> bool {
        insert_link(
            pool,
            project_id,
            fingerprint,
            integration_id,
            "gh-intg",
            "github",
            external_id,
            external_url,
            state,
            at,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn upsert_and_read_link() {
        let pool = open_test_db().await;
        let integration_id = seed_github_integration(&pool).await;

        let won = link(
            &pool,
            1,
            "fp1",
            integration_id,
            "42",
            "https://git/acme/backend/issues/42",
            Some("open"),
            1_700_000_000,
        )
        .await;
        assert!(won);

        // second insert for the same (fingerprint, integration_id) loses the race
        let won_again = link(
            &pool,
            1,
            "fp1",
            integration_id,
            "99",
            "https://other",
            None,
            1_700_000_001,
        )
        .await;
        assert!(!won_again);

        let links = links_for_issue(&pool, 1, "fp1").await.unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].external_url, "https://git/acme/backend/issues/42");
        assert_eq!(links[0].integration_kind, "github");
        assert_eq!(links[0].integration_name, "gh-intg");
        assert_eq!(links[0].external_state.as_deref(), Some("open"));
    }

    #[tokio::test]
    async fn a_link_survives_its_integration_with_name_and_kind_readable() {
        let pool = open_test_db().await;
        let integration_id = seed_github_integration(&pool).await;
        link(
            &pool,
            1,
            "fp-durable",
            integration_id,
            "7",
            "https://git/acme/backend/issues/7",
            Some("open"),
            1_700_000_000,
        )
        .await;

        crate::queries::integrations::delete_integration(&pool, integration_id, 1)
            .await
            .unwrap();

        let links = links_for_issue(&pool, 1, "fp-durable").await.unwrap();
        assert_eq!(links.len(), 1, "the link must outlive its integration");
        assert_eq!(links[0].integration_id, None, "the reference goes dangling");
        assert_eq!(links[0].integration_name, "gh-intg");
        assert_eq!(links[0].integration_kind, "github");
        assert_eq!(links[0].external_url, "https://git/acme/backend/issues/7");
    }

    #[tokio::test]
    async fn delete_link_is_scoped_to_its_project() {
        let pool = open_test_db().await;
        let integration_id = seed_github_integration(&pool).await;
        link(
            &pool,
            1,
            "fp-del",
            integration_id,
            "1",
            "https://git/a/b/issues/1",
            None,
            1_700_000_000,
        )
        .await;
        let link_id = links_for_issue(&pool, 1, "fp-del").await.unwrap()[0].id;

        assert_eq!(delete_link(&pool, 999, link_id).await.unwrap(), 0);
        assert_eq!(links_for_issue(&pool, 1, "fp-del").await.unwrap().len(), 1);

        assert_eq!(delete_link(&pool, 1, link_id).await.unwrap(), 1);
        assert!(links_for_issue(&pool, 1, "fp-del")
            .await
            .unwrap()
            .is_empty());

        assert_eq!(delete_link(&pool, 1, link_id).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn two_projects_sharing_a_fingerprint_see_only_their_own_links() {
        let pool = open_test_db().await;
        let integration_id = seed_github_integration(&pool).await;

        link(
            &pool,
            1,
            "fp-shared",
            integration_id,
            "11",
            "https://git/acme/ours/issues/11",
            None,
            1_700_000_000,
        )
        .await;
        // A different integration too, or `UNIQUE(fingerprint, integration_id)` eats the second row.
        sqlx::query(sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (2, 'other', 'Other')
             ON CONFLICT (org_id) DO NOTHING"
        ))
        .execute(&pool)
        .await
        .unwrap();
        create_integration(
            &pool,
            2,
            "gh-theirs",
            "github",
            None,
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap();
        let theirs: i64 = sqlx::query(sql!("SELECT id FROM integrations WHERE name = 'gh-theirs'"))
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        insert_link(
            &pool,
            2,
            "fp-shared",
            theirs,
            "gh-theirs",
            "github",
            "22",
            "https://git/other/theirs/issues/22",
            None,
            1_700_000_001,
        )
        .await
        .unwrap();

        let ours = links_for_issue(&pool, 1, "fp-shared").await.unwrap();
        assert_eq!(ours.len(), 1);
        assert_eq!(ours[0].external_url, "https://git/acme/ours/issues/11");

        let theirs_links = links_for_issue(&pool, 2, "fp-shared").await.unwrap();
        assert_eq!(theirs_links.len(), 1);
        assert_eq!(
            theirs_links[0].external_url,
            "https://git/other/theirs/issues/22"
        );
    }

    /// `UNIQUE(fingerprint, integration_id)` treats NULLs as distinct, so orphans coexist.
    #[tokio::test]
    async fn several_links_per_issue_including_orphans() {
        let pool = open_test_db().await;
        let first = seed_github_integration(&pool).await;
        create_integration(&pool, 1, "gh-two", "github", None, None, None, false, false)
            .await
            .unwrap();
        let second: i64 = sqlx::query(sql!("SELECT id FROM integrations WHERE name = 'gh-two'"))
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);

        link(
            &pool,
            1,
            "fp-multi",
            first,
            "1",
            "https://git/a/b/issues/1",
            None,
            1_700_000_000,
        )
        .await;
        link(
            &pool,
            1,
            "fp-multi",
            second,
            "2",
            "https://git/a/c/issues/2",
            None,
            1_700_000_001,
        )
        .await;

        crate::queries::integrations::delete_integration(&pool, first, 1)
            .await
            .unwrap();

        let links = links_for_issue(&pool, 1, "fp-multi").await.unwrap();
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].integration_id, None);
        assert_eq!(links[1].integration_id, Some(second));
    }
}
