//! `create_tracker_issue`: open an issue in a configured tracker and link it.
//! The orchestration is shared with the web handler in [`crate::trackers`].

use serde_json::{json, Value};

use super::truncate::{truncation_schema, Report};
use super::{i64_arg, internal, opt_i64_arg, prop, schema_object, str_arg, ToolCtx, ToolError};
use crate::mcp::principal::Target;
use crate::trackers::{link_issue, LinkError, LinkRequest};

pub(super) fn create_input() -> Value {
    schema_object(
        json!({
            "fingerprint": prop("string", "Issue to open a tracker issue for; see list_issues."),
            "integration_id": prop(
                "integer",
                "Tracker integration to use. Configured in the web UI under the organization's \
                 integrations; get_issue reports the ones already linked to an issue.",
            ),
            "repo_id": prop(
                "integer",
                "Which of the project's repositories to file into. Only needed when the \
                 integration can reach more than one of them, in which case calling without it \
                 fails with a message naming the candidates and their repo ids.",
            ),
        }),
        &["fingerprint", "integration_id"],
    )
}

pub(super) fn create_output() -> Value {
    schema_object(
        json!({
            "fingerprint": prop("string", "Stackpit issue the tracker issue belongs to."),
            "project_id": prop("integer", "Owning project."),
            "integration_id": prop("integer", "Integration the issue was created through."),
            "integration_name": prop("string", "Configured integration name."),
            "integration_kind": prop("string", "github, forgejo or gitlab."),
            "external_id": prop("string", "Issue id in the tracker."),
            "external_url": prop("string", "Link to the tracker issue."),
            "created": prop(
                "boolean",
                "False when this issue was already linked to that integration, in which case the \
                 existing link is returned and no tracker issue was opened.",
            ),
            "truncation": truncation_schema(),
        }),
        &[
            "fingerprint",
            "project_id",
            "integration_id",
            "external_id",
            "external_url",
            "created",
            "truncation",
        ],
    )
}

pub(super) async fn create_tracker_issue(
    ctx: &ToolCtx,
    args: &Value,
    target: Target,
) -> Result<Value, ToolError> {
    let fingerprint = str_arg(args, "fingerprint")?;
    let integration_id = i64_arg(args, "integration_id")?;
    let repo_id = opt_i64_arg(args, "repo_id")?;
    let Target::Project(project_id) = target else {
        return Err(internal("create_tracker_issue", "target is not a project"));
    };

    let org_id = crate::queries::orgs::org_of_project(&ctx.pool, project_id)
        .await
        .map_err(|e| internal("create_tracker_issue", format!("{e:#}")))?
        .ok_or_else(|| ToolError::NotFound("not found".to_string()))?;
    let issue_url = format!(
        "{}/web/projects/{project_id}/issues/{fingerprint}/",
        ctx.web_base
    );

    // The tracker is authenticated with the integration's stored secret; the MCP
    // access token never leaves Stackpit.
    let link = link_issue(
        &ctx.pool,
        &ctx.writer_pool,
        ctx.encryptor.as_deref(),
        &ctx.license,
        &LinkRequest {
            org_id,
            project_id,
            fingerprint,
            integration_id,
            repo_id,
            issue_url: &issue_url,
        },
    )
    .await
    .map_err(|e| tool_error(fingerprint, integration_id, e))?;

    // The shared write audit records the project and the outcome; an outbound
    // write to a third party is worth naming the tracker issue it opened.
    if link.created {
        tracing::info!(
            auth_source = "mcp",
            tool = "create_tracker_issue",
            user_id = ctx.principal.user_id,
            client_id = ctx.principal.client_id.as_deref().unwrap_or("-"),
            project_id,
            fingerprint,
            integration_id = link.integration_id,
            external_url = %link.external_url,
            "mcp opened a tracker issue",
        );
    }

    let mut report = Report::default();
    Ok(json!({
        "fingerprint": fingerprint,
        "project_id": project_id,
        "integration_id": link.integration_id,
        "integration_name": report.text(&link.integration_name),
        "integration_kind": link.integration_kind.as_str(),
        "external_id": report.text(&link.external_id),
        "external_url": report.text(&link.external_url),
        "created": link.created,
        "truncation": report.to_json(),
    }))
}

/// A failure the caller can act on is [`ToolError::Invalid`] with a message
/// saying what to fix; anything on the tracker's side is [`ToolError::Internal`]
/// with the cause in the log, so the model does not retry its way through an
/// outage. No tracker response body is ever carried out of here.
fn tool_error(fingerprint: &str, integration_id: i64, err: LinkError) -> ToolError {
    match err {
        LinkError::IssueNotFound => ToolError::NotFound("not found".to_string()),
        LinkError::IntegrationNotFound => ToolError::Invalid(format!(
            "no tracker integration {integration_id} in the organization that owns this project"
        )),
        LinkError::Misconfigured(m) => ToolError::Invalid(format!(
            "tracker integration {integration_id} cannot be used: {m}. Fix it in the web UI under \
             the organization's integrations."
        )),
        // Passed through whole: it names the candidate repos, which nothing else reports.
        LinkError::Ambiguous(m) => ToolError::Invalid(m),
        LinkError::Blocked(m) => ToolError::Invalid(format!(
            "tracker integration {integration_id} points somewhere Stackpit will not call: {m}"
        )),
        LinkError::Rejected(m) => ToolError::Invalid(m),
        LinkError::LicenseRequired => ToolError::Forbidden(
            "issue trackers require an active Stackpit commercial license".to_string(),
        ),
        LinkError::Unavailable(m) => {
            tracing::warn!(fingerprint, integration_id, "mcp create_tracker_issue: {m}");
            ToolError::Internal
        }
        LinkError::Internal(e) => internal("create_tracker_issue", format!("{e:#}")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{call, seed_org, seed_project};
    use super::ToolError;
    use crate::mcp::principal::McpPrincipal;
    use crate::mcp::{SCOPE_EVENTS_READ, SCOPE_PROJECTS_WRITE};
    use crate::orgs::Role;
    use crate::queries::integrations::create_integration;
    use crate::queries::test_helpers::insert_test_issue;
    use serde_json::json;

    async fn seed(pool: &crate::db::DbPool, slug: &str, project_id: i64, fingerprint: &str) -> i64 {
        let org = seed_org(pool, slug).await;
        seed_project(pool, project_id, org, slug).await;
        insert_test_issue(
            pool,
            fingerprint,
            project_id,
            Some("boom"),
            Some("error"),
            1_000,
            2_000,
            3,
            "unresolved",
        )
        .await;
        org
    }

    async fn tracker(pool: &crate::db::DbPool, org_id: i64, name: &str) -> i64 {
        create_integration(
            pool,
            org_id,
            name,
            "github",
            Some("https://git.invalid"),
            Some("tok"),
            Some(r#"{"owner":"acme","repo":"backend"}"#),
            false,
            false,
        )
        .await
        .unwrap()
    }

    async fn repo(pool: &crate::db::DbPool, project_id: i64, url: &str) -> i64 {
        crate::queries::projects::upsert_project_repo(
            pool,
            project_id as u64,
            url,
            "github",
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

    async fn links(pool: &crate::db::DbPool, project_id: i64, fingerprint: &str) -> usize {
        crate::queries::issue_links::links_for_issue(pool, project_id, fingerprint)
            .await
            .unwrap()
            .len()
    }

    #[tokio::test]
    async fn a_missing_or_ill_typed_argument_is_a_tool_error() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "trk-args", 6001, "fp-args").await;
        let integration = tracker(&pool, org, "gh-args").await;

        for args in [
            json!({ "integration_id": integration }),
            json!({ "fingerprint": "fp-args" }),
            json!({ "fingerprint": "fp-args", "integration_id": "one" }),
            json!({ "fingerprint": "", "integration_id": integration }),
        ] {
            let err = call(
                &pool,
                McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(org, Role::Owner)]),
                "create_tracker_issue",
                args.clone(),
            )
            .await
            .expect_err("bad arguments");
            assert!(
                matches!(err, ToolError::Invalid(_) | ToolError::NotFound(_)),
                "{args} produced {err:?}"
            );
        }
        assert_eq!(links(&pool, 6001, "fp-args").await, 0);
    }

    // A misconfigured or absent integration is the caller's to fix, so it comes
    // back as an actionable tool error rather than a generic internal one.
    #[tokio::test]
    async fn an_unknown_integration_is_reported_as_actionable() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "trk-unknown", 6010, "fp-unknown").await;

        let owner = McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            owner,
            "create_tracker_issue",
            json!({ "fingerprint": "fp-unknown", "integration_id": 4242 }),
        )
        .await
        .expect_err("no such integration");
        let ToolError::Invalid(msg) = err else {
            panic!("expected an actionable tool error, got {err:?}");
        };
        assert!(msg.contains("4242"), "{msg}");
    }

    #[tokio::test]
    async fn an_integration_without_a_credential_is_reported_as_actionable() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "trk-nocred", 6020, "fp-nocred").await;
        let integration = create_integration(
            &pool,
            org,
            "gh-nocred",
            "github",
            Some("https://git.invalid"),
            None,
            Some(r#"{"owner":"acme","repo":"backend"}"#),
            false,
            false,
        )
        .await
        .unwrap();

        let owner = McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            owner,
            "create_tracker_issue",
            json!({ "fingerprint": "fp-nocred", "integration_id": integration }),
        )
        .await
        .expect_err("no stored credential");
        assert!(matches!(err, ToolError::Invalid(_)), "{err:?}");
        assert_eq!(links(&pool, 6020, "fp-nocred").await, 0);
    }

    // A tracker integration belongs to an org; one from another org must not be
    // reachable even when the caller owns the project they name.
    #[tokio::test]
    async fn an_integration_from_another_org_is_not_usable() {
        let pool = crate::db::open_test_pool().await;
        let mine = seed(&pool, "trk-mine", 6030, "fp-mine").await;
        let theirs = seed_org(&pool, "trk-theirs").await;
        let foreign = tracker(&pool, theirs, "gh-theirs").await;

        let owner = McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(mine, Role::Owner)]);
        let err = call(
            &pool,
            owner,
            "create_tracker_issue",
            json!({ "fingerprint": "fp-mine", "integration_id": foreign }),
        )
        .await
        .expect_err("another org's integration");
        assert!(matches!(err, ToolError::Invalid(_)), "{err:?}");
        assert_eq!(links(&pool, 6030, "fp-mine").await, 0);
    }

    #[tokio::test]
    async fn an_ambiguous_call_names_the_candidate_repos() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "trk-amb", 6070, "fp-amb").await;
        let integration = tracker(&pool, org, "gh-amb").await;
        repo(&pool, 6070, "https://git.invalid/acme/api").await;
        repo(&pool, 6070, "https://git.invalid/acme/web").await;

        let owner = McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            owner,
            "create_tracker_issue",
            json!({ "fingerprint": "fp-amb", "integration_id": integration }),
        )
        .await
        .expect_err("two candidates, none chosen");

        match err {
            ToolError::Invalid(m) => {
                assert!(m.contains("acme/api"), "{m}");
                assert!(m.contains("acme/web"), "{m}");
                assert!(
                    m.contains("repo_id"),
                    "the message must name the argument: {m}"
                );
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(links(&pool, 6070, "fp-amb").await, 0);
    }

    /// Past target selection the call dies on the network, which is as far as a test can drive it.
    #[tokio::test]
    async fn a_repo_id_selects_among_the_candidates() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "trk-pick", 6071, "fp-pick").await;
        let integration = tracker(&pool, org, "gh-pick").await;
        repo(&pool, 6071, "https://git.invalid/acme/api").await;
        let web = repo(&pool, 6071, "https://git.invalid/acme/web").await;

        let owner = McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            owner,
            "create_tracker_issue",
            json!({
                "fingerprint": "fp-pick",
                "integration_id": integration,
                "repo_id": web,
            }),
        )
        .await
        .expect_err("git.invalid does not resolve");

        let msg = format!("{err:?}");
        assert!(
            !msg.contains("repo_id") && !msg.contains("several repositories"),
            "the choice should have been accepted, got {msg}"
        );
    }

    #[tokio::test]
    async fn an_ill_typed_repo_id_is_a_tool_error() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "trk-badrepo", 6072, "fp-badrepo").await;
        let integration = tracker(&pool, org, "gh-badrepo").await;
        repo(&pool, 6072, "https://git.invalid/acme/api").await;

        let owner = McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            owner,
            "create_tracker_issue",
            json!({
                "fingerprint": "fp-badrepo",
                "integration_id": integration,
                "repo_id": "one",
            }),
        )
        .await
        .expect_err("repo_id must be an integer");
        assert!(matches!(err, ToolError::Invalid(_)), "{err:?}");
    }

    // Already linked: the existing link is the answer, and no second tracker
    // issue is opened. Also the one path that returns success without a network
    // call, so it is where the result shape gets asserted.
    #[tokio::test]
    async fn an_already_linked_issue_returns_the_existing_link() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "trk-linked", 6040, "fp-linked").await;
        let integration = tracker(&pool, org, "gh-linked").await;
        crate::queries::issue_links::insert_link(
            &pool,
            6040,
            "fp-linked",
            integration,
            "gh-linked",
            "github",
            "11",
            "https://git.invalid/acme/backend/issues/11",
            Some("open"),
            1_700_000_000,
        )
        .await
        .unwrap();

        let owner = McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            owner,
            "create_tracker_issue",
            json!({ "fingerprint": "fp-linked", "integration_id": integration }),
        )
        .await
        .unwrap();

        assert_eq!(out["created"], false);
        assert_eq!(out["external_id"], "11");
        assert_eq!(
            out["external_url"],
            "https://git.invalid/acme/backend/issues/11"
        );
        assert_eq!(out["integration_kind"], "github");
        assert_eq!(out["project_id"], 6040);
        assert_eq!(out["truncation"]["truncated"], false);
        assert_eq!(links(&pool, 6040, "fp-linked").await, 1);
    }

    // The web UI requires the owner role to link a tracker issue.
    #[tokio::test]
    async fn a_member_cannot_create_a_tracker_issue() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "trk-member", 6050, "fp-member").await;
        let integration = tracker(&pool, org, "gh-member").await;

        let member = McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(org, Role::Member)]);
        let err = call(
            &pool,
            member,
            "create_tracker_issue",
            json!({ "fingerprint": "fp-member", "integration_id": integration }),
        )
        .await
        .expect_err("members cannot link tracker issues");
        assert!(matches!(err, ToolError::Forbidden(_)), "{err:?}");
        assert_eq!(
            links(&pool, 6050, "fp-member").await,
            0,
            "no link was written"
        );
    }

    #[tokio::test]
    async fn an_issue_in_a_foreign_org_is_not_found() {
        let pool = crate::db::open_test_pool().await;
        let theirs = seed(&pool, "trk-foreign", 6060, "fp-foreign").await;
        let integration = tracker(&pool, theirs, "gh-foreign").await;
        let mine = seed_org(&pool, "trk-outsider").await;

        let outsider = McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(mine, Role::Owner)]);
        let err = call(
            &pool,
            outsider,
            "create_tracker_issue",
            json!({ "fingerprint": "fp-foreign", "integration_id": integration }),
        )
        .await
        .expect_err("a foreign issue is not reachable");
        assert_eq!(err, ToolError::NotFound("not found".to_string()));
        assert_eq!(links(&pool, 6060, "fp-foreign").await, 0);
    }

    #[tokio::test]
    async fn without_the_write_scope_it_steps_up() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "trk-noscope", 6070, "fp-noscope").await;
        let integration = tracker(&pool, org, "gh-noscope").await;

        let reader = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            reader,
            "create_tracker_issue",
            json!({ "fingerprint": "fp-noscope", "integration_id": integration }),
        )
        .await
        .expect_err("projects:write is required");
        assert_eq!(
            err,
            ToolError::Scope {
                required: SCOPE_PROJECTS_WRITE
            }
        );
        assert_eq!(links(&pool, 6070, "fp-noscope").await, 0);
    }
}
