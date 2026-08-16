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
    /// Which repository to file into; only needed when the integration matches more than one.
    pub repo_id: Option<i64>,
    /// Deep link back to the Stackpit issue, carried in the tracker issue body.
    pub issue_url: &'a str,
}

/// One integration paired with one repository it can file into.
pub struct TrackerMatch {
    pub integration: queries::Integration,
    pub repo: queries::ProjectRepo,
    pub forge_ref: crate::forge::ForgeRef,
}

/// GitHub's API host (`api.github.com`) isn't its repo host; self-hosted forges share one host.
fn hosts_match(integration_host: &str, repo_host: &str) -> bool {
    if integration_host.eq_ignore_ascii_case(repo_host) {
        return true;
    }
    let strip_api = |h: &str| h.strip_prefix("api.").map(str::to_ascii_lowercase);
    strip_api(integration_host).is_some_and(|h| h == repo_host.to_ascii_lowercase())
        || strip_api(repo_host).is_some_and(|h| h == integration_host.to_ascii_lowercase())
}

/// Coordinates for filing this integration into this repo, or `None` if the pair can't be used.
pub fn tracker_repo_match(
    integration: &queries::Integration,
    repo: &queries::ProjectRepo,
) -> Option<crate::forge::ForgeRef> {
    let tag = crate::forge::tracker_forge_tag(integration.kind)?;
    if repo.effective_forge_type() != tag {
        return None;
    }
    let url = integration.url.as_deref()?;
    if !hosts_match(
        &crate::forge::extract_hostname(url),
        &crate::forge::extract_hostname(&repo.repo_url),
    ) {
        return None;
    }
    let forge = crate::forge::ForgeType::from_tag(tag);
    crate::forge::derive_forge_ref(&forge, &repo.repo_url).ok()
}

/// Every (tracker integration, repository) pair that can file for this project.
///
/// Matches on forge kind *and* host: one instance's token must never be used against another.
pub async fn resolve_matching_trackers(
    pool: &DbPool,
    org_id: i64,
    project_id: i64,
) -> anyhow::Result<Vec<TrackerMatch>> {
    let repos = queries::projects::get_project_repos(pool, project_id as u64).await?;
    if repos.is_empty() {
        return Ok(Vec::new());
    }

    let excluded = queries::integration_exclusions::excluded_ids(pool, project_id).await?;
    let integrations = queries::integrations::list_integrations(pool, Some(org_id)).await?;

    let mut matches = Vec::new();
    for integration in integrations {
        if !integration.kind.is_tracker() || excluded.contains(&integration.id) {
            continue;
        }
        for repo in &repos {
            let Some(forge_ref) = tracker_repo_match(&integration, repo) else {
                continue;
            };
            matches.push(TrackerMatch {
                integration: integration.clone(),
                repo: repo.clone(),
                forge_ref,
            });
        }
    }
    Ok(matches)
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
    /// a project repository it can address issues in.
    Misconfigured(String),
    /// Several repositories match and the caller named none; the message lists the candidates.
    Ambiguous(String),
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
            LinkError::Ambiguous(m) => write!(f, "{m}"),
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

    let existing = queries::issue_links::links_for_issue(pool, req.project_id, req.fingerprint)
        .await
        .map_err(LinkError::Internal)?
        .into_iter()
        .find(|l| l.integration_id == Some(req.integration_id));
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

    let matches = resolve_matching_trackers(pool, req.org_id, req.project_id)
        .await
        .map_err(LinkError::Internal)?
        .into_iter()
        .filter(|m| m.integration.id == req.integration_id)
        .collect::<Vec<_>>();

    let chosen = match (req.repo_id, matches.len()) {
        (_, 0) => {
            return Err(LinkError::Misconfigured(
                "no repository on this project matches it; set the project's repository \
                 under project settings"
                    .to_string(),
            ))
        }
        (None, 1) => matches.into_iter().next().expect("length checked"),
        (None, _) => {
            let candidates = matches
                .iter()
                .map(|m| format!("{} (repo_id {})", m.repo.repo_url, m.repo.id))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(LinkError::Ambiguous(format!(
                "this project has several repositories it could file into; \
                 pick one of: {candidates}"
            )));
        }
        (Some(repo_id), _) => match matches.into_iter().find(|m| m.repo.id == repo_id) {
            Some(m) => m,
            None => {
                return Err(LinkError::Misconfigured(
                    "the chosen repository is not one this integration can file into".to_string(),
                ))
            }
        },
    };

    let target = TrackerTarget {
        base_url: base_url.to_string(),
        owner: chosen.forge_ref.owner.clone(),
        repo: chosen.forge_ref.repo.clone(),
        gitlab_path: chosen.forge_ref.gitlab_path.clone(),
    };

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
        &integration.name,
        integration.kind.as_str(),
        &created.external_id,
        &created.external_url,
        created.external_state.as_deref(),
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
        request_for(project_id, fingerprint, integration_id, None)
    }

    fn request_for<'a>(
        project_id: i64,
        fingerprint: &'a str,
        integration_id: i64,
        repo_id: Option<i64>,
    ) -> LinkRequest<'a> {
        LinkRequest {
            org_id: 1,
            project_id,
            fingerprint,
            integration_id,
            repo_id,
            issue_url: "https://stackpit.test/web/projects/1/issues/fp/",
        }
    }

    async fn repo(pool: &DbPool, project_id: i64, url: &str) -> i64 {
        let (forge, _) = crate::forge::detect_forge(url);
        crate::queries::projects::upsert_project_repo(
            pool,
            project_id as u64,
            url,
            forge.as_str(),
            None,
            None,
            None,
        )
        .await
        .unwrap();
        crate::queries::projects::get_project_repos(pool, project_id as u64)
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.repo_url == url)
            .expect("just inserted")
            .id
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
            false,
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
            "gh-dup",
            "github",
            "7",
            "https://git.test/acme/backend/issues/7",
            Some("open"),
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
    fn github_api_host_matches_the_repo_host() {
        assert!(hosts_match("api.github.com", "github.com"));
        assert!(hosts_match("github.com", "api.github.com"));
        assert!(hosts_match("git.gofranz.com", "git.gofranz.com"));
        assert!(hosts_match("GitHub.com", "github.com"));
    }

    /// One org's token must never be used against a different instance of the same forge.
    #[test]
    fn a_different_host_on_the_same_forge_does_not_match() {
        assert!(!hosts_match("api.github.com", "ghe.acme.internal"));
        assert!(!hosts_match("gitlab.acme.internal", "gitlab.com"));
    }

    #[tokio::test]
    async fn matching_pairs_each_integration_with_each_same_host_repo() {
        let pool = crate::db::open_test_pool().await;
        let gh = integration(&pool, 1, "gh", "github", Some("https://api.github.com")).await;
        integration(&pool, 1, "gl", "gitlab", Some("https://gitlab.com")).await;

        let api = repo(&pool, 1, "https://github.com/acme/api").await;
        let web = repo(&pool, 1, "https://github.com/acme/web").await;
        repo(&pool, 1, "https://bitbucket.org/acme/other").await;

        let matches = resolve_matching_trackers(&pool, 1, 1).await.unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|m| m.integration.id == gh));
        let mut ids: Vec<i64> = matches.iter().map(|m| m.repo.id).collect();
        ids.sort();
        assert_eq!(ids, vec![api, web]);
    }

    #[tokio::test]
    async fn an_excluded_integration_never_matches() {
        let pool = crate::db::open_test_pool().await;
        let gh = integration(&pool, 1, "gh-ex", "github", Some("https://api.github.com")).await;
        repo(&pool, 1, "https://github.com/acme/api").await;
        assert_eq!(
            resolve_matching_trackers(&pool, 1, 1).await.unwrap().len(),
            1
        );

        sqlx::query(crate::db::sql!(
            "INSERT INTO integration_exclusions (org_id, integration_id, project_id) \
             VALUES (1, ?1, 1)"
        ))
        .bind(gh)
        .execute(&pool)
        .await
        .unwrap();

        assert!(resolve_matching_trackers(&pool, 1, 1)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_repo_url_without_a_usable_path_does_not_match() {
        let pool = crate::db::open_test_pool().await;
        integration(&pool, 1, "gh-bad", "github", Some("https://api.github.com")).await;
        repo(&pool, 1, "https://github.com/acme").await;

        assert!(resolve_matching_trackers(&pool, 1, 1)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_forge_override_makes_an_undetectable_host_match() {
        let pool = crate::db::open_test_pool().await;
        integration(
            &pool,
            1,
            "fj",
            "forgejo",
            Some("https://git.gofranz.com/api/v1"),
        )
        .await;
        crate::queries::projects::upsert_project_repo(
            &pool,
            1,
            "https://git.gofranz.com/franz/stackpit",
            "unknown",
            None,
            None,
            None,
        )
        .await
        .unwrap();
        assert!(resolve_matching_trackers(&pool, 1, 1)
            .await
            .unwrap()
            .is_empty());

        crate::queries::projects::upsert_project_repo(
            &pool,
            1,
            "https://git.gofranz.com/franz/stackpit",
            "unknown",
            Some("gitea"),
            None,
            None,
        )
        .await
        .unwrap();

        let matches = resolve_matching_trackers(&pool, 1, 1).await.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].forge_ref.owner.as_deref(), Some("franz"));
        assert_eq!(matches[0].forge_ref.repo.as_deref(), Some("stackpit"));
    }

    #[tokio::test]
    async fn no_matching_repo_is_a_misconfiguration_naming_project_settings() {
        let pool = crate::db::open_test_pool().await;
        let id = integration(
            &pool,
            1,
            "gh-norepo",
            "github",
            Some("https://api.github.com"),
        )
        .await;
        issue(&pool, "fp-norepo", 1).await;

        let err = link_issue(
            &pool,
            &pool,
            None,
            &licensed(),
            &request(1, "fp-norepo", id),
        )
        .await
        .expect_err("nothing to file into");
        match err {
            LinkError::Misconfigured(m) => assert!(m.contains("repositor"), "{m}"),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn a_single_match_is_picked_without_a_repo_id() {
        let pool = crate::db::open_test_pool().await;
        let id = integration(&pool, 1, "gh-one", "github", Some("https://api.github.com")).await;
        repo(&pool, 1, "https://github.com/acme/api").await;
        issue(&pool, "fp-one", 1).await;

        // Resolution succeeds, so the call gets as far as the network - which tests don't have.
        let err = link_issue(&pool, &pool, None, &licensed(), &request(1, "fp-one", id))
            .await
            .expect_err("no outbound network in tests");
        assert!(
            !matches!(err, LinkError::Misconfigured(_) | LinkError::Ambiguous(_)),
            "target resolution should have succeeded, got {err:?}"
        );
    }

    #[tokio::test]
    async fn several_matches_without_a_repo_id_are_ambiguous_and_name_the_candidates() {
        let pool = crate::db::open_test_pool().await;
        let id = integration(
            &pool,
            1,
            "gh-many",
            "github",
            Some("https://api.github.com"),
        )
        .await;
        repo(&pool, 1, "https://github.com/acme/api").await;
        repo(&pool, 1, "https://github.com/acme/web").await;
        issue(&pool, "fp-many", 1).await;

        let err = link_issue(&pool, &pool, None, &licensed(), &request(1, "fp-many", id))
            .await
            .expect_err("two repos, no choice made");
        match err {
            LinkError::Ambiguous(m) => {
                assert!(m.contains("acme/api"), "{m}");
                assert!(m.contains("acme/web"), "{m}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn an_ambiguous_call_with_a_valid_repo_id_resolves() {
        let pool = crate::db::open_test_pool().await;
        let id = integration(
            &pool,
            1,
            "gh-pick",
            "github",
            Some("https://api.github.com"),
        )
        .await;
        repo(&pool, 1, "https://github.com/acme/api").await;
        let web = repo(&pool, 1, "https://github.com/acme/web").await;
        issue(&pool, "fp-pick", 1).await;

        let err = link_issue(
            &pool,
            &pool,
            None,
            &licensed(),
            &request_for(1, "fp-pick", id, Some(web)),
        )
        .await
        .expect_err("no outbound network in tests");
        assert!(
            !matches!(err, LinkError::Misconfigured(_) | LinkError::Ambiguous(_)),
            "the named repo should have resolved, got {err:?}"
        );
    }

    #[tokio::test]
    async fn a_repo_id_from_another_project_is_rejected() {
        let pool = crate::db::open_test_pool().await;
        let id = integration(
            &pool,
            1,
            "gh-other",
            "github",
            Some("https://api.github.com"),
        )
        .await;
        repo(&pool, 1, "https://github.com/acme/api").await;
        let elsewhere = repo(&pool, 2, "https://github.com/acme/secret").await;
        issue(&pool, "fp-other", 1).await;

        let err = link_issue(
            &pool,
            &pool,
            None,
            &licensed(),
            &request_for(1, "fp-other", id, Some(elsewhere)),
        )
        .await
        .expect_err("that repo belongs to another project");
        assert!(matches!(err, LinkError::Misconfigured(_)), "{err:?}");
    }
}
