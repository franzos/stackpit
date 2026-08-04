//! `whoami`: what this connection is authorized as. Exists so an auth failure
//! is diagnosable from inside the protocol rather than from the server log.

use serde_json::{json, Value};

use super::{internal, prop, schema_object, ToolCtx, ToolError};
use crate::mcp::principal::Target;

pub(super) fn whoami_input() -> Value {
    schema_object(json!({}), &[])
}

pub(super) fn whoami_output() -> Value {
    schema_object(
        json!({
            "iss": prop("string", "Issuer that minted the access token."),
            "sub": prop("string", "Subject identifier at the issuer."),
            "user_id": prop("integer", "Stackpit user id."),
            "client_id": {
                "type": ["string", "null"],
                "description": "OAuth client that presented the token.",
            },
            "scopes": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Scopes granted to this token.",
            },
            "organizations": {
                "type": "array",
                "description": "Organizations reachable from this token, with your role in each.",
                "items": schema_object(
                    json!({
                        "org_id": prop("integer", "Organization id."),
                        "slug": prop("string", "Organization slug."),
                        "name": { "type": ["string", "null"] },
                        "role": prop("string", "owner or member."),
                        "is_personal": prop("boolean", "Auto-created personal organization."),
                    }),
                    &["org_id", "slug", "role"],
                ),
            },
        }),
        &["iss", "sub", "user_id", "scopes", "organizations"],
    )
}

pub(super) async fn whoami(
    ctx: &ToolCtx,
    _args: &Value,
    _target: Target,
) -> Result<Value, ToolError> {
    let principal = &ctx.principal;
    // Membership rows supply the labels; the reachable set stays whatever
    // `accessible_org_ids` says it is.
    let accessible = principal.accessible_org_ids();
    let memberships = crate::queries::orgs::list_memberships(&ctx.pool, principal.user_id)
        .await
        .map_err(|e| internal("whoami", format!("{e:#}")))?;

    let organizations: Vec<Value> = memberships
        .iter()
        .filter(|m| accessible.contains(&m.org_id))
        .map(|m| {
            json!({
                "org_id": m.org_id,
                "slug": m.slug,
                "name": m.name,
                "role": crate::orgs::Role::parse(&m.role).as_str(),
                "is_personal": m.is_personal,
            })
        })
        .collect();

    Ok(json!({
        "iss": principal.iss,
        "sub": principal.sub,
        "user_id": principal.user_id,
        "client_id": principal.client_id,
        "scopes": principal.scopes.as_slice(),
        "organizations": organizations,
    }))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{call, seed_org};
    use crate::mcp::principal::McpPrincipal;
    use crate::orgs::{Role, SYSTEM_ORG_ID};
    use serde_json::json;

    async fn seed_user(pool: &crate::db::DbPool) -> i64 {
        crate::queries::users::upsert_from_oidc(pool, "https://idp.test", "alice", None, None)
            .await
            .unwrap()
            .user_id
    }

    #[tokio::test]
    async fn whoami_reports_the_token_identity_and_its_orgs() {
        let pool = crate::db::open_test_pool().await;
        let user_id = seed_user(&pool).await;
        let org = seed_org(&pool, "whoami-org").await;
        crate::queries::orgs::add_member(&pool, user_id, org, Role::Owner)
            .await
            .unwrap();

        let mut principal =
            McpPrincipal::for_test("stackpit:events:read", vec![(org, Role::Owner)]);
        principal.user_id = user_id;

        let out = call(&pool, principal, "whoami", json!({})).await.unwrap();
        assert_eq!(out["sub"], "alice");
        assert_eq!(out["user_id"], user_id);
        assert_eq!(out["client_id"], "mcp-client");
        assert_eq!(out["scopes"], json!(["stackpit:events:read"]));
        assert_eq!(out["organizations"].as_array().unwrap().len(), 1);
        assert_eq!(out["organizations"][0]["org_id"], org);
        assert_eq!(out["organizations"][0]["role"], "owner");
    }

    // `whoami` carries no scope requirement: a token that can do nothing else
    // must still be able to find out why.
    #[tokio::test]
    async fn whoami_needs_no_scope() {
        let pool = crate::db::open_test_pool().await;
        let out = call(
            &pool,
            McpPrincipal::for_test("", Vec::new()),
            "whoami",
            json!({}),
        )
        .await
        .unwrap();
        assert_eq!(out["organizations"], json!([]));
        assert_eq!(out["scopes"], json!([]));
    }

    // A stray membership row for the system org must not surface as an org the
    // caller can reach.
    #[tokio::test]
    async fn whoami_never_lists_the_system_org() {
        let pool = crate::db::open_test_pool().await;
        let user_id = seed_user(&pool).await;
        crate::queries::orgs::add_member(&pool, user_id, SYSTEM_ORG_ID, Role::Owner)
            .await
            .unwrap();

        let mut principal = McpPrincipal::for_test("", vec![(SYSTEM_ORG_ID, Role::Owner)]);
        principal.user_id = user_id;

        let out = call(&pool, principal, "whoami", json!({})).await.unwrap();
        assert_eq!(out["organizations"], json!([]));
    }
}
