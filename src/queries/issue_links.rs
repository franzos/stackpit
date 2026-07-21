use crate::db::{sql, DbPool};
use sqlx::Row;

pub struct ExternalLink {
    pub integration_id: i64,
    pub integration_kind: String,
    pub integration_name: String,
    pub external_id: String,
    pub external_url: String,
    pub external_state: Option<String>,
}

/// Insert-first idempotency guard: returns true if THIS insert created the row,
/// false if a link for (fingerprint, integration_id) already existed.
pub async fn insert_link(
    pool: &DbPool,
    project_id: i64,
    fingerprint: &str,
    integration_id: i64,
    external_id: &str,
    external_url: &str,
    created_at: i64,
) -> anyhow::Result<bool> {
    let res = sqlx::query(sql!(
        "INSERT INTO issue_external_links \
         (project_id, fingerprint, integration_id, external_id, external_url, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT (fingerprint, integration_id) DO NOTHING"
    ))
    .bind(project_id)
    .bind(fingerprint)
    .bind(integration_id)
    .bind(external_id)
    .bind(external_url)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() == 1)
}

pub async fn link_exists(
    pool: &DbPool,
    fingerprint: &str,
    integration_id: i64,
) -> anyhow::Result<bool> {
    let row = sqlx::query(sql!(
        "SELECT 1 FROM issue_external_links WHERE fingerprint = ?1 AND integration_id = ?2"
    ))
    .bind(fingerprint)
    .bind(integration_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

pub async fn links_for_issue(
    pool: &DbPool,
    fingerprint: &str,
) -> anyhow::Result<Vec<ExternalLink>> {
    let rows = sqlx::query(sql!(
        "SELECT l.integration_id, i.kind AS integration_kind, i.name AS integration_name, \
                l.external_id, l.external_url, l.external_state \
         FROM issue_external_links l \
         JOIN integrations i ON i.id = l.integration_id \
         WHERE l.fingerprint = ?1 ORDER BY l.created_at"
    ))
    .bind(fingerprint)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ExternalLink {
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
        create_integration(pool, 1, "gh-intg", "github", None, None, None, false)
            .await
            .unwrap();
        sqlx::query(sql!("SELECT id FROM integrations WHERE name = 'gh-intg'"))
            .fetch_one(pool)
            .await
            .unwrap()
            .get(0)
    }

    #[tokio::test]
    async fn upsert_and_read_link() {
        let pool = open_test_db().await;
        let integration_id = seed_github_integration(&pool).await;

        let won = insert_link(
            &pool,
            1,
            "fp1",
            integration_id,
            "42",
            "https://git/acme/backend/issues/42",
            1_700_000_000,
        )
        .await
        .unwrap();
        assert!(won);

        // second insert for the same (fingerprint, integration_id) loses the race
        let won_again = insert_link(
            &pool,
            1,
            "fp1",
            integration_id,
            "99",
            "https://other",
            1_700_000_001,
        )
        .await
        .unwrap();
        assert!(!won_again);

        assert!(link_exists(&pool, "fp1", integration_id).await.unwrap());

        let links = links_for_issue(&pool, "fp1").await.unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].external_url, "https://git/acme/backend/issues/42");
        assert_eq!(links[0].integration_kind, "github");
        assert!(links[0].external_state.is_none());
    }
}
