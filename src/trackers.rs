//! Creating an issue in an external tracker and linking it to a Stackpit issue.
//!
//! Shared by the web handler and the MCP tool: resolving the integration, its
//! credential and its target, the outbound call and the stored link all live
//! here, so a fix lands once. How a failure is presented (flash message, tool
//! error) stays with the caller.

use crate::commercial::providers::tracker::{
    create_issue, issue_api_url, NewExternalIssue, TrackerError, TrackerTarget,
};
use crate::commercial::LicenseHandle;
use crate::db::DbPool;
use crate::domain::IntegrationKind;
use crate::queries;
use crate::util::crypto::SecretEncryptor;
use crate::util::ssrf::{build_pinned_client, check_ssrf};

pub struct LinkRequest<'a> {
    /// Org that owns the project; scopes the integration lookup.
    pub org_id: i64,
    pub project_id: i64,
    pub fingerprint: &'a str,
    pub integration_id: i64,
    /// Deep link back to the Stackpit issue, carried in the tracker issue body.
    pub issue_url: &'a str,
}

#[derive(Debug)]
pub struct LinkedIssue {
    pub integration_id: i64,
    pub integration_name: String,
    pub integration_kind: IntegrationKind,
    pub external_id: String,
    pub external_url: String,
    /// False when a link already existed, so no tracker call was made.
    pub created: bool,
}

/// Why no issue was created. Split by who can act on it: the first four are the
/// caller's to fix, the last two are not.
#[derive(Debug)]
pub enum LinkError {
    IssueNotFound,
    /// Absent, owned by another org, or not a tracker kind.
    IntegrationNotFound,
    /// The integration is missing something it needs: credential, base URL, or
    /// the owner/repo/project the tracker addresses issues by.
    Misconfigured(String),
    /// The tracker URL resolves somewhere Stackpit must not reach.
    Blocked(String),
    /// The tracker refused the request as sent (4xx).
    Rejected(String),
    /// The tracker is unreachable, failing, or answered with nonsense.
    Unavailable(String),
    /// Issue trackers are gated behind `Feature::Integrations`.
    LicenseRequired,
    Internal(anyhow::Error),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkError::IssueNotFound => f.write_str("issue not found"),
            LinkError::IntegrationNotFound => f.write_str("integration not found"),
            LinkError::Misconfigured(m) => write!(f, "integration is misconfigured: {m}"),
            LinkError::Blocked(m) => write!(f, "tracker URL refused: {m}"),
            LinkError::Rejected(m) => write!(f, "{m}"),
            LinkError::Unavailable(m) => write!(f, "{m}"),
            LinkError::LicenseRequired => {
                f.write_str("issue trackers require an active commercial license")
            }
            LinkError::Internal(e) => write!(f, "{e:#}"),
        }
    }
}

/// Create the tracker issue and store the link, or hand back the link that
/// already exists.
///
/// The tracker is authenticated with the integration's own stored secret. No
/// credential presented by the caller (web session cookie, MCP access token)
/// ever reaches the tracker: this is the boundary where a confused deputy would
/// otherwise be born.
pub async fn link_issue(
    pool: &DbPool,
    writer_pool: &DbPool,
    encryptor: Option<&SecretEncryptor>,
    license: &LicenseHandle,
    req: &LinkRequest<'_>,
) -> Result<LinkedIssue, LinkError> {
    let integration =
        queries::integrations::get_integration(pool, req.integration_id, Some(req.org_id))
            .await
            .map_err(LinkError::Internal)?;
    let integration = match integration {
        Some(i) if i.kind.is_tracker() => i,
        _ => return Err(LinkError::IntegrationNotFound),
    };
    // Creating a remote issue is a hard POST with an external side effect, so
    // grace doesn't cover it (see `FeatureStatus::GraceReadOnly`).
    if !crate::commercial::providers::may_configure(license, integration.kind) {
        return Err(LinkError::LicenseRequired);
    }

    let existing = queries::issue_links::links_for_issue(pool, req.fingerprint)
        .await
        .map_err(LinkError::Internal)?
        .into_iter()
        .find(|l| l.integration_id == req.integration_id);
    if let Some(link) = existing {
        return Ok(LinkedIssue {
            integration_id: integration.id,
            integration_name: integration.name,
            integration_kind: integration.kind,
            external_id: link.external_id,
            external_url: link.external_url,
            created: false,
        });
    }

    let issue = queries::issues::get_issue(pool, req.fingerprint)
        .await
        .map_err(LinkError::Internal)?
        .ok_or(LinkError::IssueNotFound)?;
    if issue.project_id as i64 != req.project_id {
        return Err(LinkError::IssueNotFound);
    }

    let token = match (&integration.secret, integration.encrypted, encryptor) {
        (Some(s), true, Some(enc)) => enc.decrypt(s),
        (Some(s), false, _) => Some(s.clone()),
        _ => None,
    };
    let Some(token) = token else {
        return Err(LinkError::Misconfigured(
            "no usable credential is stored for it".to_string(),
        ));
    };
    let Some(base_url) = integration.url.as_deref() else {
        return Err(LinkError::Misconfigured("it has no base URL".to_string()));
    };

    let default_target: serde_json::Value = integration
        .config
        .as_deref()
        .and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let project_override =
        queries::tracker_targets::get_override(pool, req.project_id, integration.id)
            .await
            .map_err(LinkError::Internal)?;
    let target = resolve_target(base_url, &default_target, project_override.as_ref());

    let url = issue_api_url(integration.kind, &target).map_err(|e| match e {
        TrackerError::Config(m) => LinkError::Misconfigured(m),
        other => LinkError::Unavailable(other.to_string()),
    })?;
    let resolved = check_ssrf(&url).await.map_err(LinkError::Blocked)?;
    let client = build_pinned_client(&resolved).map_err(LinkError::Internal)?;

    let title = issue.title.unwrap_or_else(|| req.fingerprint.to_string());
    let body = format!("{title}\n\n{}", req.issue_url);
    let created = create_issue(
        &client,
        integration.kind,
        &target,
        &token,
        &NewExternalIssue {
            title: &title,
            body: &body,
        },
    )
    .await
    .map_err(|e| match e {
        TrackerError::Rejected(status) => LinkError::Rejected(format!(
            "the tracker rejected the request with HTTP {}; check the integration's \
             credential and its owner/repo or project target",
            status.as_u16()
        )),
        TrackerError::Config(m) => LinkError::Misconfigured(m),
        other => LinkError::Unavailable(other.to_string()),
    })?;

    let now = chrono::Utc::now().timestamp();
    if let Err(e) = queries::issue_links::insert_link(
        writer_pool,
        req.project_id,
        req.fingerprint,
        integration.id,
        &created.external_id,
        &created.external_url,
        now,
    )
    .await
    {
        // The tracker issue exists either way; a concurrent request may have won.
        tracing::warn!(
            integration_id = integration.id,
            fingerprint = req.fingerprint,
            "tracker link could not be stored: {e:#}"
        );
    }

    Ok(LinkedIssue {
        integration_id: integration.id,
        integration_name: integration.name,
        integration_kind: integration.kind,
        external_id: created.external_id,
        external_url: created.external_url,
        created: true,
    })
}

/// Resolves the tracker target: each field (owner, repo, project_id) is taken
/// from the per-project override if present there, otherwise falls back to the
/// integration's default target; base_url always comes from `integrations.url`,
/// not from either target object.
pub fn resolve_target(
    base_url: &str,
    default_target: &serde_json::Value,
    project_override: Option<&serde_json::Value>,
) -> TrackerTarget {
    let field_str = |key: &str| {
        project_override
            .and_then(|o| o.get(key))
            .and_then(|v| v.as_str())
            .or_else(|| default_target.get(key).and_then(|v| v.as_str()))
            .map(String::from)
    };
    let project_id = project_override
        .and_then(|o| o.get("project_id"))
        .and_then(|v| v.as_i64())
        .or_else(|| default_target.get("project_id").and_then(|v| v.as_i64()));
    TrackerTarget {
        base_url: base_url.to_string(),
        owner: field_str("owner"),
        repo: field_str("repo"),
        project_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::integrations::create_integration;
    use crate::queries::test_helpers::insert_test_issue;

    async fn integration(
        pool: &DbPool,
        org_id: i64,
        name: &str,
        kind: &str,
        url: Option<&str>,
    ) -> i64 {
        create_integration(
            pool,
            org_id,
            name,
            kind,
            url,
            Some("tok"),
            Some(r#"{"owner":"acme","repo":"backend"}"#),
            false,
        )
        .await
        .unwrap()
    }

    async fn issue(pool: &DbPool, fingerprint: &str, project_id: i64) {
        insert_test_issue(
            pool,
            fingerprint,
            project_id,
            Some("boom"),
            Some("error"),
            1_000,
            2_000,
            1,
            "unresolved",
        )
        .await;
    }

    fn request<'a>(project_id: i64, fingerprint: &'a str, integration_id: i64) -> LinkRequest<'a> {
        LinkRequest {
            org_id: 1,
            project_id,
            fingerprint,
            integration_id,
            issue_url: "https://stackpit.test/web/projects/1/issues/fp/",
        }
    }

    fn licensed() -> LicenseHandle {
        crate::commercial::fully_licensed()
    }

    // The gate sits ahead of every other failure mode, so an unlicensed install
    // is refused before the tracker is ever contacted.
    #[tokio::test]
    async fn an_unlicensed_install_refuses_to_create_a_tracker_issue() {
        let pool = crate::db::open_test_pool().await;
        let id = integration(
            &pool,
            1,
            "gh-unlicensed",
            "github",
            Some("https://git.test"),
        )
        .await;
        issue(&pool, "fp-unlicensed", 1).await;

        let unlicensed = LicenseHandle::new(
            crate::commercial::LicenseStatus::Unlicensed,
            crate::commercial::GRACE_DAYS,
        );
        let err = link_issue(
            &pool,
            &pool,
            None,
            &unlicensed,
            &request(1, "fp-unlicensed", id),
        )
        .await
        .expect_err("trackers are gated behind Feature::Integrations");
        assert!(matches!(err, LinkError::LicenseRequired), "{err:?}");
    }

    #[tokio::test]
    async fn a_non_tracker_integration_is_not_found() {
        let pool = crate::db::open_test_pool().await;
        let id = integration(&pool, 1, "hook", "webhook", Some("https://example.test")).await;
        issue(&pool, "fp-kind", 1).await;

        let err = link_issue(&pool, &pool, None, &licensed(), &request(1, "fp-kind", id))
            .await
            .expect_err("a webhook is not a tracker");
        assert!(matches!(err, LinkError::IntegrationNotFound), "{err:?}");
    }

    // The integration lookup is org-scoped, so another org's tracker is absent.
    #[tokio::test]
    async fn an_integration_in_another_org_is_not_found() {
        let pool = crate::db::open_test_pool().await;
        // Only org 1 comes from migrations; postgres enforces the FK, sqlite doesn't.
        sqlx::query(crate::db::sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (?1, ?2, ?2)
             ON CONFLICT(org_id) DO NOTHING"
        ))
        .bind(2i64)
        .bind("org-2")
        .execute(&pool)
        .await
        .unwrap();
        let id = integration(&pool, 2, "gh-other", "github", Some("https://git.test")).await;
        issue(&pool, "fp-org", 1).await;

        let err = link_issue(&pool, &pool, None, &licensed(), &request(1, "fp-org", id))
            .await
            .expect_err("a foreign org's integration is not reachable");
        assert!(matches!(err, LinkError::IntegrationNotFound), "{err:?}");
    }

    #[tokio::test]
    async fn a_fingerprint_from_another_project_is_not_found() {
        let pool = crate::db::open_test_pool().await;
        let id = integration(&pool, 1, "gh-fp", "github", Some("https://git.test")).await;
        issue(&pool, "fp-elsewhere", 42).await;

        let err = link_issue(
            &pool,
            &pool,
            None,
            &licensed(),
            &request(1, "fp-elsewhere", id),
        )
        .await
        .expect_err("the fingerprint belongs to another project");
        assert!(matches!(err, LinkError::IssueNotFound), "{err:?}");
    }

    #[tokio::test]
    async fn an_integration_without_a_base_url_is_misconfigured() {
        let pool = crate::db::open_test_pool().await;
        let id = integration(&pool, 1, "gh-nourl", "github", None).await;
        issue(&pool, "fp-nourl", 1).await;

        let err = link_issue(&pool, &pool, None, &licensed(), &request(1, "fp-nourl", id))
            .await
            .expect_err("no base URL");
        assert!(matches!(err, LinkError::Misconfigured(_)), "{err:?}");
    }

    // An encrypted secret with no encryptor configured cannot be decrypted; that
    // is a configuration problem, not a tracker failure.
    #[tokio::test]
    async fn an_undecryptable_secret_is_misconfigured() {
        let pool = crate::db::open_test_pool().await;
        let id = create_integration(
            &pool,
            1,
            "gh-enc",
            "github",
            Some("https://git.test"),
            Some("ciphertext"),
            None,
            true,
        )
        .await
        .unwrap();
        issue(&pool, "fp-enc", 1).await;

        let err = link_issue(&pool, &pool, None, &licensed(), &request(1, "fp-enc", id))
            .await
            .expect_err("no encryptor to decrypt with");
        assert!(matches!(err, LinkError::Misconfigured(_)), "{err:?}");
    }

    // Missing owner/repo has to be named as configuration, not reported as a
    // tracker outage: nobody is going to retry their way out of it.
    #[tokio::test]
    async fn a_target_without_owner_or_repo_is_misconfigured() {
        let pool = crate::db::open_test_pool().await;
        let id = create_integration(
            &pool,
            1,
            "gh-notarget",
            "github",
            Some("https://git.test"),
            Some("tok"),
            None,
            false,
        )
        .await
        .unwrap();
        issue(&pool, "fp-notarget", 1).await;

        let err = link_issue(
            &pool,
            &pool,
            None,
            &licensed(),
            &request(1, "fp-notarget", id),
        )
        .await
        .expect_err("no owner/repo to address");
        assert!(matches!(err, LinkError::Misconfigured(_)), "{err:?}");
    }

    // An issue already linked to this integration is handed back untouched, so a
    // retry cannot open a second tracker issue.
    #[tokio::test]
    async fn an_existing_link_is_returned_without_calling_the_tracker() {
        let pool = crate::db::open_test_pool().await;
        let id = integration(&pool, 1, "gh-dup", "github", Some("https://git.test")).await;
        issue(&pool, "fp-dup", 1).await;
        queries::issue_links::insert_link(
            &pool,
            1,
            "fp-dup",
            id,
            "7",
            "https://git.test/acme/backend/issues/7",
            1_700_000_000,
        )
        .await
        .unwrap();

        let link = link_issue(&pool, &pool, None, &licensed(), &request(1, "fp-dup", id))
            .await
            .expect("the existing link is the answer");
        assert!(!link.created);
        assert_eq!(link.external_id, "7");
        assert_eq!(link.external_url, "https://git.test/acme/backend/issues/7");
    }

    #[test]
    fn resolve_target_prefers_project_override() {
        let default_cfg = serde_json::json!({ "owner": "acme", "repo": "default" });
        let override_tgt = serde_json::json!({ "owner": "acme", "repo": "frontend" });
        let t = resolve_target("https://api.github.com", &default_cfg, Some(&override_tgt));
        assert_eq!(t.repo.as_deref(), Some("frontend"));
        assert_eq!(t.base_url, "https://api.github.com");

        let t2 = resolve_target("https://api.github.com", &default_cfg, None);
        assert_eq!(t2.repo.as_deref(), Some("default"));
    }

    #[test]
    fn resolve_target_merges_partial_override_with_default() {
        let default_cfg = serde_json::json!({ "owner": "acme", "repo": "default" });
        let repo_only_override = serde_json::json!({ "repo": "frontend" });
        let t = resolve_target(
            "https://api.github.com",
            &default_cfg,
            Some(&repo_only_override),
        );
        assert_eq!(t.owner.as_deref(), Some("acme"));
        assert_eq!(t.repo.as_deref(), Some("frontend"));
    }

    #[test]
    fn resolve_target_empty_override_yields_full_default() {
        let default_cfg = serde_json::json!({ "owner": "acme", "repo": "default" });
        let empty_override = serde_json::json!({});
        let t = resolve_target(
            "https://api.github.com",
            &default_cfg,
            Some(&empty_override),
        );
        assert_eq!(t.owner.as_deref(), Some("acme"));
        assert_eq!(t.repo.as_deref(), Some("default"));
    }

    #[test]
    fn resolve_target_full_override_wins() {
        let default_cfg = serde_json::json!({ "owner": "acme", "repo": "default" });
        let full_override = serde_json::json!({ "owner": "other", "repo": "frontend" });
        let t = resolve_target("https://api.github.com", &default_cfg, Some(&full_override));
        assert_eq!(t.owner.as_deref(), Some("other"));
        assert_eq!(t.repo.as_deref(), Some("frontend"));
    }
}
