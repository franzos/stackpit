use anyhow::Result;

use crate::db::DbPool;
use crate::oidc::client::OrgClaim;
use crate::orgs::Role;
use crate::queries::orgs::*;

pub const FORSETI_DEFAULT_ORG_ID: &str = "default";

pub struct ReconcileInput<'a> {
    pub user_id: i64,
    pub iss: &'a str,
    /// None = claim absent (scope not granted) = zero authority = no removals.
    pub orgs: Option<&'a [OrgClaim]>,
    pub orgs_truncated: bool,
}

#[derive(Default)]
pub struct ReconcileResult {
    pub provisionable: Vec<OrgClaim>,
}

pub async fn reconcile(pool: &DbPool, input: ReconcileInput<'_>) -> Result<ReconcileResult> {
    // Personal org first: removals can never strand the user with zero orgs.
    ensure_personal_org(pool, input.user_id).await?;

    let mut provisionable = Vec::new();
    // Absent claim (None) = scope not granted = no removals or downgrades.
    let removals_allowed = input.orgs.is_some() && !input.orgs_truncated;

    let claim = input.orgs.unwrap_or(&[]);
    let mut seen_org_ids = std::collections::HashSet::new();

    for oc in claim {
        if oc.id == FORSETI_DEFAULT_ORG_ID {
            continue; // "default" maps to the personal org, already ensured above
        }
        seen_org_ids.insert(oc.id.clone());
        let role = Role::parse(&oc.role);
        match org_by_ext(pool, input.iss, &oc.id).await? {
            Some(org_id) => {
                // DO NOTHING insert: never overwrites an existing role before the last-owner guard.
                add_member(pool, input.user_id, org_id, role).await?;
                if role_sync_enabled(pool, org_id).await? {
                    // guarded: promote is unconditional, demote refuses the sole owner (atomic)
                    set_member_role_guarded(pool, input.user_id, org_id, role).await?;
                }
            }
            None if role == Role::Owner => provisionable.push(oc.clone()),
            None => {} // member of not-yet-provisioned org: wait for an owner claim
        }
    }

    if removals_allowed {
        for m in list_memberships(pool, input.user_id).await? {
            // Only Forseti-backed orgs from this issuer; other issuers' orgs not covered by this claim.
            if m.ext_iss.as_deref() != Some(input.iss) {
                continue;
            }
            let Some(ext) = m.ext_org_id.as_deref() else {
                continue;
            };
            if !seen_org_ids.contains(ext) {
                // guarded: refuses to remove the sole owner (atomic)
                remove_member_guarded(pool, input.user_id, m.org_id).await?;
            }
        }
    }

    Ok(ReconcileResult { provisionable })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_user(pool: &DbPool) -> i64 {
        crate::queries::users::upsert_from_oidc(pool, "https://idp", "seed-user", None, None)
            .await
            .unwrap()
            .user_id
    }

    async fn seed_user2(pool: &DbPool) -> i64 {
        crate::queries::users::upsert_from_oidc(pool, "https://idp", "seed-user-2", None, None)
            .await
            .unwrap()
            .user_id
    }

    fn acme_member_claim() -> OrgClaim {
        OrgClaim { id: "acme".into(), slug: "acme".into(), role: "member".into(), name: None }
    }

    fn acme_owner_claim() -> OrgClaim {
        OrgClaim { id: "acme".into(), slug: "acme".into(), role: "owner".into(), name: None }
    }

    async fn role_for(pool: &DbPool, user_id: i64, org_id: i64) -> String {
        list_memberships(pool, user_id)
            .await
            .unwrap()
            .into_iter()
            .find(|m| m.org_id == org_id)
            .map(|m| m.role)
            .unwrap()
    }

    #[tokio::test]
    async fn absent_claim_never_removes() {
        let pool = crate::db::open_test_pool().await;
        let u = seed_user(&pool).await;
        // second owner so the gate, not the guard, must retain the membership
        let u2 = seed_user2(&pool).await;
        let org =
            provision_forseti_org(&pool, u, "https://idp", "acme", "acme", "Acme").await.unwrap();
        add_member(&pool, u2, org, Role::Owner).await.unwrap();
        reconcile(
            &pool,
            ReconcileInput { user_id: u, iss: "https://idp", orgs: None, orgs_truncated: false },
        )
        .await
        .unwrap();
        assert!(list_memberships(&pool, u).await.unwrap().iter().any(|m| m.org_id == org));
    }

    #[tokio::test]
    async fn truncated_claim_skips_removals() {
        let pool = crate::db::open_test_pool().await;
        let u = seed_user(&pool).await;
        // second owner so the gate, not the guard, must retain the membership
        let u2 = seed_user2(&pool).await;
        let org =
            provision_forseti_org(&pool, u, "https://idp", "acme", "acme", "Acme").await.unwrap();
        add_member(&pool, u2, org, Role::Owner).await.unwrap();
        reconcile(
            &pool,
            ReconcileInput {
                user_id: u,
                iss: "https://idp",
                orgs: Some(&[]),
                orgs_truncated: true,
            },
        )
        .await
        .unwrap();
        assert!(list_memberships(&pool, u).await.unwrap().iter().any(|m| m.org_id == org));
    }

    #[tokio::test]
    async fn present_empty_claim_removes_when_safe() {
        let pool = crate::db::open_test_pool().await;
        let u = seed_user(&pool).await;
        // second owner so removal does not hit the last-owner guard
        let u2 = crate::queries::users::upsert_from_oidc(&pool, "https://idp", "sub2", None, None)
            .await
            .unwrap()
            .user_id;
        let org =
            provision_forseti_org(&pool, u, "https://idp", "acme", "acme", "Acme").await.unwrap();
        add_member(&pool, u2, org, Role::Owner).await.unwrap();
        reconcile(
            &pool,
            ReconcileInput {
                user_id: u,
                iss: "https://idp",
                orgs: Some(&[]),
                orgs_truncated: false,
            },
        )
        .await
        .unwrap();
        assert!(!list_memberships(&pool, u).await.unwrap().iter().any(|m| m.org_id == org));
    }

    #[tokio::test]
    async fn last_owner_is_never_removed() {
        let pool = crate::db::open_test_pool().await;
        let u = seed_user(&pool).await;
        let org =
            provision_forseti_org(&pool, u, "https://idp", "acme", "acme", "Acme").await.unwrap();
        reconcile(
            &pool,
            ReconcileInput {
                user_id: u,
                iss: "https://idp",
                orgs: Some(&[]),
                orgs_truncated: false,
            },
        )
        .await
        .unwrap();
        // retained: u is the last owner
        assert!(list_memberships(&pool, u).await.unwrap().iter().any(|m| m.org_id == org));
    }

    #[tokio::test]
    async fn unknown_org_owner_is_provisionable_member_is_not() {
        let pool = crate::db::open_test_pool().await;
        let u = seed_user(&pool).await;
        let owner_claim =
            OrgClaim { id: "new".into(), slug: "new".into(), role: "owner".into(), name: Some("New".into()) };
        let member_claim =
            OrgClaim { id: "other".into(), slug: "other".into(), role: "member".into(), name: None };
        let r = reconcile(
            &pool,
            ReconcileInput {
                user_id: u,
                iss: "https://idp",
                orgs: Some(&[owner_claim, member_claim]),
                orgs_truncated: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(r.provisionable.len(), 1);
        assert_eq!(r.provisionable[0].id, "new");
    }

    #[tokio::test]
    async fn default_org_id_skipped_no_extra_membership() {
        let pool = crate::db::open_test_pool().await;
        let u = seed_user(&pool).await;
        let default_claim =
            OrgClaim { id: "default".into(), slug: "default".into(), role: "member".into(), name: None };
        reconcile(
            &pool,
            ReconcileInput {
                user_id: u,
                iss: "https://idp",
                orgs: Some(&[default_claim]),
                orgs_truncated: false,
            },
        )
        .await
        .unwrap();
        let ms = list_memberships(&pool, u).await.unwrap();
        // Only the personal org; "default" sentinel is not provisioned as a Forseti org.
        assert_eq!(ms.len(), 1);
        assert!(ms[0].is_personal);
    }

    // Gap 1a: two owners; claim downgrades to member → set_member_role executes.
    #[tokio::test]
    async fn role_sync_downgrade_allowed_with_two_owners() {
        let pool = crate::db::open_test_pool().await;
        let u = seed_user(&pool).await;
        let u2 = seed_user2(&pool).await;
        let org =
            provision_forseti_org(&pool, u, "https://idp", "acme", "acme", "Acme").await.unwrap();
        add_member(&pool, u2, org, Role::Owner).await.unwrap();
        reconcile(
            &pool,
            ReconcileInput {
                user_id: u,
                iss: "https://idp",
                orgs: Some(&[acme_member_claim()]),
                orgs_truncated: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(role_for(&pool, u, org).await, "member");
    }

    // Gap 1b: sole owner; claim requests member → last-owner guard retains owner.
    #[tokio::test]
    async fn role_sync_last_owner_blocks_downgrade() {
        let pool = crate::db::open_test_pool().await;
        let u = seed_user(&pool).await;
        let org =
            provision_forseti_org(&pool, u, "https://idp", "acme", "acme", "Acme").await.unwrap();
        reconcile(
            &pool,
            ReconcileInput {
                user_id: u,
                iss: "https://idp",
                orgs: Some(&[acme_member_claim()]),
                orgs_truncated: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(role_for(&pool, u, org).await, "owner");
    }

    // Gap 1c: member promoted to owner via claim.
    #[tokio::test]
    async fn role_sync_member_promoted_to_owner() {
        let pool = crate::db::open_test_pool().await;
        let u = seed_user(&pool).await;
        let u2 = seed_user2(&pool).await;
        let org =
            provision_forseti_org(&pool, u2, "https://idp", "acme", "acme", "Acme").await.unwrap();
        add_member(&pool, u, org, Role::Member).await.unwrap();
        reconcile(
            &pool,
            ReconcileInput {
                user_id: u,
                iss: "https://idp",
                orgs: Some(&[acme_owner_claim()]),
                orgs_truncated: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(role_for(&pool, u, org).await, "owner");
    }
}
