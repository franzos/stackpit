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

impl ExternalLink {
    /// Which repository this link points at, in the same shape
    /// [`trackers::repo_key`](crate::trackers::repo_key) produces.
    ///
    /// New rows carry it as the prefix of the qualified `external_id`. Rows
    /// filed before that key existed carry a bare `"42"`, so it is recovered
    /// from `external_url` instead — comparing the *repository* rather than the
    /// id string is what makes legacy and new rows behave identically. A
    /// string comparison against the newly computed key would miss every
    /// pre-migration link and open a second issue on the operator's forge.
    ///
    /// `None` when the URL is hand-edited or otherwise unparseable. Callers
    /// fail toward offering the target: a duplicate issue can be closed,
    /// whereas hiding the only reachable target is the bug being fixed.
    pub fn repo_key(&self) -> Option<String> {
        if let Some((prefix, _)) = self.external_id.rsplit_once('#') {
            if !prefix.is_empty() {
                return Some(prefix.to_string());
            }
        }
        repo_key_from_url(&self.external_url)
    }

    /// The forge's own issue number, without the repository qualifier.
    pub fn issue_number(&self) -> &str {
        match self.external_id.rsplit_once('#') {
            Some((_, n)) if !n.is_empty() => n,
            _ => &self.external_id,
        }
    }

    /// `open` / `closed`, normalising the forge spellings. GitLab says
    /// `opened`; GitHub and Forgejo say `open`. An unrecognised value renders
    /// no badge rather than a wrong one.
    pub fn normalised_state(&self) -> Option<&'static str> {
        match self.external_state.as_deref()?.trim() {
            "open" | "opened" | "reopened" => Some("open"),
            "closed" | "merged" | "resolved" => Some("closed"),
            _ => None,
        }
    }

    /// Fluent key for the state badge.
    pub fn state_label_key(&self) -> Option<&'static str> {
        match self.normalised_state()? {
            "closed" => Some("issue-detail-external-state-closed"),
            _ => Some("issue-detail-external-state-open"),
        }
    }

    pub fn state_is_closed(&self) -> bool {
        self.normalised_state() == Some("closed")
    }
}

/// Recover `owner/repo` (or GitLab's encoded namespace path) from an issue URL.
/// GitHub and Forgejo are `…/owner/repo/issues/42`; GitLab is
/// `…/group/sub/project/-/issues/42`.
fn repo_key_from_url(url: &str) -> Option<String> {
    let after_scheme = url.find("://").map(|i| &url[i + 3..]).unwrap_or(url);
    let (_, path) = after_scheme.split_once('/')?;
    let path = path.trim_matches('/');

    // GitLab's `/-/` separates the namespace from the resource, however deep.
    if let Some((namespace, _)) = path.split_once("/-/") {
        if namespace.is_empty() {
            return None;
        }
        return Some(
            percent_encoding::utf8_percent_encode(namespace, crate::forge::GITLAB_PATH_ENCODE)
                .to_string(),
        );
    }

    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let issues_at = segments.iter().rposition(|s| *s == "issues")?;
    if issues_at < 2 {
        return None;
    }
    Some(format!(
        "{}/{}",
        segments[issues_at - 2],
        segments[issues_at - 1]
    ))
}

/// Insert-first idempotency guard: returns true if THIS insert created the row,
/// false if a link for (fingerprint, integration_id, external_id) already
/// existed. The key carries the external issue's identity, so one issue can be
/// filed into two repositories of the same integration.
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
         ON CONFLICT (fingerprint, integration_id, external_id) DO NOTHING"
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

        // A different external issue on the same integration is a different
        // link: one Stackpit issue can be filed into two repositories of one
        // forge. The old `UNIQUE(fingerprint, integration_id)` made this lose.
        let second = link(
            &pool,
            1,
            "fp1",
            integration_id,
            "99",
            "https://git/acme/frontend/issues/99",
            None,
            1_700_000_001,
        )
        .await;
        assert!(second, "a second repository is a second link");

        // The same external issue twice is still idempotent.
        let duplicate = link(
            &pool,
            1,
            "fp1",
            integration_id,
            "42",
            "https://git/acme/backend/issues/42",
            Some("closed"),
            1_700_000_002,
        )
        .await;
        assert!(!duplicate, "the same triple is still rejected");

        let links = links_for_issue(&pool, 1, "fp1").await.unwrap();
        assert_eq!(links.len(), 2);
        let first = links
            .iter()
            .find(|l| l.external_id == "42")
            .expect("the original link");
        assert_eq!(first.external_url, "https://git/acme/backend/issues/42");
        assert_eq!(first.integration_kind, "github");
        assert_eq!(first.integration_name, "gh-intg");
        assert_eq!(
            first.external_state.as_deref(),
            Some("open"),
            "the duplicate insert did not overwrite the state"
        );
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
        // A second integration, so the scoping is tested across integrations
        // and not merely across external ids.
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

    /// `UNIQUE(fingerprint, integration_id, external_id)` treats NULLs as
    /// distinct, so orphaned links — all carrying `integration_id IS NULL` —
    /// coexist rather than collapsing onto one row.
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

    fn a_link(external_id: &str, external_url: &str) -> ExternalLink {
        ExternalLink {
            id: 1,
            integration_id: Some(1),
            integration_kind: "github".into(),
            integration_name: "gh".into(),
            external_id: external_id.into(),
            external_url: external_url.into(),
            external_state: None,
        }
    }

    #[test]
    fn repo_key_prefers_the_qualified_id_and_falls_back_to_the_url() {
        // New shape: the key is already in the id.
        assert_eq!(
            a_link("acme/backend#42", "https://git.test/acme/backend/issues/42")
                .repo_key()
                .as_deref(),
            Some("acme/backend")
        );

        // Legacy GitHub/Forgejo shape: recovered from the URL.
        assert_eq!(
            a_link("42", "https://github.com/acme/backend/issues/42")
                .repo_key()
                .as_deref(),
            Some("acme/backend")
        );

        // Legacy GitLab shape: `/-/` separates the namespace, however deep.
        assert_eq!(
            a_link("42", "https://gitlab.com/group/sub/project/-/issues/42")
                .repo_key()
                .as_deref(),
            Some("group%2Fsub%2Fproject")
        );

        // Unparseable: callers fail toward offering the target.
        assert!(a_link("42", "not a url at all").repo_key().is_none());
        assert!(a_link("42", "https://git.test/").repo_key().is_none());
        assert!(a_link("42", "https://git.test/issues/42")
            .repo_key()
            .is_none());
    }

    #[test]
    fn issue_number_strips_the_qualifier() {
        assert_eq!(a_link("acme/backend#42", "u").issue_number(), "42");
        assert_eq!(a_link("42", "u").issue_number(), "42");
        // GitLab's encoded key contains no '#', so only the last one splits.
        assert_eq!(a_link("group%2Fsub#7", "u").issue_number(), "7");
    }

    #[test]
    fn external_state_is_normalised_across_forges() {
        let mut l = a_link("1", "u");
        for (raw, want) in [
            ("open", Some("open")),
            ("opened", Some("open")),
            ("reopened", Some("open")),
            ("closed", Some("closed")),
            ("merged", Some("closed")),
            (" open ", Some("open")),
            ("something-else", None),
        ] {
            l.external_state = Some(raw.into());
            assert_eq!(l.normalised_state(), want, "{raw}");
        }
        l.external_state = None;
        assert_eq!(l.normalised_state(), None);
        assert!(!l.state_is_closed());
    }
}
