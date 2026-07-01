// Org/membership query helpers; writes are idempotent or guarded.

use anyhow::Result;
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::db::{sql, DbPool};
use crate::orgs::{Role, SYSTEM_ORG_ID};

pub struct Membership {
    pub org_id: i64,
    pub role: String,
    pub slug: String,
    pub name: Option<String>,
    pub is_personal: bool,
    /// Required by the reconcile removal loop, which filters by issuer.
    pub ext_iss: Option<String>,
    pub ext_org_id: Option<String>,
}

async fn personal_org_id(pool: &DbPool, user_id: i64) -> Result<Option<i64>> {
    let row = sqlx::query(sql!(
        "SELECT org_id FROM organizations WHERE created_by = ?1 AND is_personal = ?2"
    ))
    .bind(user_id)
    .bind(true)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.get("org_id")))
}

async fn ensure_owner_membership(pool: &DbPool, user_id: i64, org_id: i64) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(sql!(
        "INSERT INTO organization_members (user_id, org_id, role, joined_at) \
         VALUES (?1, ?2, 'owner', ?3) ON CONFLICT (user_id, org_id) DO NOTHING"
    ))
    .bind(user_id)
    .bind(org_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

// Idempotent: an existing personal org is returned untouched so a renamed slug survives re-login; slug is a neutral random token deduped on collision.
pub async fn ensure_personal_org(pool: &DbPool, user_id: i64) -> Result<i64> {
    if let Some(org_id) = personal_org_id(pool, user_id).await? {
        ensure_owner_membership(pool, user_id, org_id).await?;
        return Ok(org_id);
    }

    let base = format!("personal-{}", crate::util::crypto::random_hex::<3>());

    #[cfg(feature = "sqlite")]
    let ins = "INSERT OR IGNORE INTO organizations (slug, name, created_by, is_personal) \
               VALUES (?1, 'Personal', ?2, 1)";
    #[cfg(not(feature = "sqlite"))]
    let ins = "INSERT INTO organizations (slug, name, created_by, is_personal) \
               VALUES (?1, 'Personal', ?2, TRUE) ON CONFLICT DO NOTHING";
    let translated = crate::db::translate_sql(ins);

    for attempt in 0u32..100 {
        let candidate = if attempt == 0 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };

        let result = sqlx::query(translated.as_ref())
            .bind(&candidate)
            .bind(user_id)
            .execute(pool)
            .await?;

        if result.rows_affected() > 0 {
            let org_id = personal_org_id(pool, user_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("personal org vanished after insert"))?;
            ensure_owner_membership(pool, user_id, org_id).await?;
            return Ok(org_id);
        }

        // rows_affected == 0: a concurrent first login won the (created_by) race, or a pure slug collision.
        if let Some(org_id) = personal_org_id(pool, user_id).await? {
            ensure_owner_membership(pool, user_id, org_id).await?;
            return Ok(org_id);
        }
        // Pure slug collision; advance the suffix.
    }

    anyhow::bail!("ensure_personal_org: slug {base:?} exhausted 100 suffix attempts")
}

pub async fn list_memberships(pool: &DbPool, user_id: i64) -> Result<Vec<Membership>> {
    let rows = sqlx::query(sql!(
        "SELECT m.org_id, m.role, o.slug, o.name, o.is_personal, o.ext_iss, o.ext_org_id \
         FROM organization_members m \
         JOIN organizations o ON o.org_id = m.org_id \
         WHERE m.user_id = ?1"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Membership {
            org_id: r.get("org_id"),
            role: r.get("role"),
            slug: r.get("slug"),
            name: r.get("name"),
            is_personal: r.get::<bool, _>("is_personal"),
            ext_iss: r.get("ext_iss"),
            ext_org_id: r.get("ext_org_id"),
        })
        .collect())
}

pub async fn org_by_ext(pool: &DbPool, ext_iss: &str, ext_org_id: &str) -> Result<Option<i64>> {
    let row = sqlx::query(sql!(
        "SELECT org_id FROM organizations WHERE ext_iss = ?1 AND ext_org_id = ?2"
    ))
    .bind(ext_iss)
    .bind(ext_org_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.get("org_id")))
}

// DO NOTHING: never overwrites existing role; use set_member_role for that.
pub async fn add_member(pool: &DbPool, user_id: i64, org_id: i64, role: Role) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(sql!(
        "INSERT INTO organization_members (user_id, org_id, role, joined_at) \
         VALUES (?1, ?2, ?3, ?4) ON CONFLICT (user_id, org_id) DO NOTHING"
    ))
    .bind(user_id)
    .bind(org_id)
    .bind(role.as_str())
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_member_role(pool: &DbPool, user_id: i64, org_id: i64, role: Role) -> Result<()> {
    sqlx::query(sql!(
        "UPDATE organization_members SET role = ?1 WHERE user_id = ?2 AND org_id = ?3"
    ))
    .bind(role.as_str())
    .bind(user_id)
    .bind(org_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_member(pool: &DbPool, user_id: i64, org_id: i64) -> Result<()> {
    sqlx::query(sql!(
        "DELETE FROM organization_members WHERE user_id = ?1 AND org_id = ?2"
    ))
    .bind(user_id)
    .bind(org_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn count_owners(pool: &DbPool, org_id: i64) -> Result<i64> {
    let row = sqlx::query(sql!(
        "SELECT COUNT(*) AS cnt FROM organization_members \
         WHERE org_id = ?1 AND role = 'owner'"
    ))
    .bind(org_id)
    .fetch_one(pool)
    .await?;
    Ok(row.get("cnt"))
}

pub async fn is_current_owner(pool: &DbPool, user_id: i64, org_id: i64) -> Result<bool> {
    let row = sqlx::query(sql!(
        "SELECT role FROM organization_members WHERE user_id = ?1 AND org_id = ?2"
    ))
    .bind(user_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map_or(false, |r| r.get::<String, _>("role") == "owner"))
}

pub async fn role_sync_enabled(pool: &DbPool, org_id: i64) -> Result<bool> {
    let row = sqlx::query(sql!(
        "SELECT role_sync FROM organizations WHERE org_id = ?1"
    ))
    .bind(org_id)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<bool, _>("role_sync"))
}

// Inserts with ext link and role_sync=true; retries slug with suffix on collision.
pub async fn provision_forseti_org(
    pool: &DbPool,
    user_id: i64,
    ext_iss: &str,
    ext_org_id: &str,
    slug: &str,
    name: &str,
) -> Result<i64> {
    if let Some(org_id) = org_by_ext(pool, ext_iss, ext_org_id).await? {
        add_member(pool, user_id, org_id, Role::Owner).await?;
        return Ok(org_id);
    }

    #[cfg(feature = "sqlite")]
    let ins = "INSERT OR IGNORE INTO organizations \
               (slug, name, ext_iss, ext_org_id, created_by, role_sync) \
               VALUES (?1, ?2, ?3, ?4, ?5, 1)";
    #[cfg(not(feature = "sqlite"))]
    let ins = "INSERT INTO organizations \
               (slug, name, ext_iss, ext_org_id, created_by, role_sync) \
               VALUES (?1, ?2, ?3, ?4, ?5, TRUE) ON CONFLICT (slug) DO NOTHING";

    let translated = crate::db::translate_sql(ins);

    for attempt in 0u32..100 {
        let candidate = if attempt == 0 {
            slug.to_owned()
        } else {
            format!("{slug}-{attempt}")
        };

        let result = sqlx::query(translated.as_ref())
            .bind(&candidate)
            .bind(name)
            .bind(ext_iss)
            .bind(ext_org_id)
            .bind(user_id)
            .execute(pool)
            .await?;

        if result.rows_affected() > 0 {
            let row = sqlx::query(sql!(
                "SELECT org_id FROM organizations WHERE ext_iss = ?1 AND ext_org_id = ?2"
            ))
            .bind(ext_iss)
            .bind(ext_org_id)
            .fetch_one(pool)
            .await?;
            let org_id: i64 = row.get("org_id");
            add_member(pool, user_id, org_id, Role::Owner).await?;
            return Ok(org_id);
        }

        // rows_affected == 0: slug taken, or ext key raced in concurrently.
        if let Some(org_id) = org_by_ext(pool, ext_iss, ext_org_id).await? {
            add_member(pool, user_id, org_id, Role::Owner).await?;
            return Ok(org_id);
        }
        // Pure slug collision; try next suffix.
    }

    anyhow::bail!("provision_forseti_org: slug {slug:?} exhausted 100 suffix attempts")
}

pub struct InvitePreview {
    pub org_name: Option<String>,
    pub org_slug: String,
    pub role: String,
    pub expires_at: i64,
    pub accepted_at: Option<i64>,
}

fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Creates an invite for a native (non-Forseti, non-system) org. Returns the raw token.
pub async fn create_invite(
    pool: &DbPool,
    org_id: i64,
    role: Role,
    email: Option<&str>,
    created_by: i64,
    ttl_secs: i64,
) -> Result<String> {
    if org_id == SYSTEM_ORG_ID {
        anyhow::bail!("cannot create invite for the system org");
    }

    let row = sqlx::query(sql!(
        "SELECT ext_org_id FROM organizations WHERE org_id = ?1"
    ))
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| anyhow::anyhow!("org {org_id} not found"))?;
    let ext_org_id: Option<String> = row.get("ext_org_id");
    if ext_org_id.is_some() {
        anyhow::bail!("cannot create invite for a Forseti-backed org");
    }

    let now = chrono::Utc::now().timestamp();
    let token = crate::util::crypto::random_hex::<32>();
    let hash = token_hash(&token);
    let expires_at = now + ttl_secs;

    sqlx::query(sql!(
        "INSERT INTO invites (org_id, role, token_hash, email, created_by, created_at, expires_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
    ))
    .bind(org_id)
    .bind(role.as_str())
    .bind(&hash)
    .bind(email)
    .bind(created_by)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(token)
}

/// Accepts an invite: adds membership with the invite role and marks the row accepted.
/// Returns the org_id on success.
pub async fn accept_invite(pool: &DbPool, token: &str, user_id: i64) -> Result<i64> {
    let hash = token_hash(token);
    let now = chrono::Utc::now().timestamp();

    let row = sqlx::query(sql!(
        "SELECT invite_id, org_id, role, expires_at, accepted_at \
         FROM invites WHERE token_hash = ?1"
    ))
    .bind(&hash)
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| anyhow::anyhow!("invite not found or invalid"))?;
    let invite_id: i64 = row.get("invite_id");
    let org_id: i64 = row.get("org_id");
    let role_str: String = row.get("role");
    let expires_at: i64 = row.get("expires_at");
    let accepted_at: Option<i64> = row.get("accepted_at");

    if accepted_at.is_some() {
        anyhow::bail!("invite already accepted");
    }
    if now > expires_at {
        anyhow::bail!("invite expired");
    }
    if org_id == SYSTEM_ORG_ID {
        anyhow::bail!("cannot accept invite for the system org");
    }

    let role = Role::parse(&role_str);
    add_member(pool, user_id, org_id, role).await?;

    sqlx::query(sql!(
        "UPDATE invites SET accepted_by = ?1, accepted_at = ?2 WHERE invite_id = ?3"
    ))
    .bind(user_id)
    .bind(now)
    .bind(invite_id)
    .execute(pool)
    .await?;

    Ok(org_id)
}

/// Fetches invite display data by raw token for the accept page. Does not modify state.
pub async fn get_invite_preview(pool: &DbPool, token: &str) -> Result<Option<InvitePreview>> {
    let hash = token_hash(token);

    let row = sqlx::query(sql!(
        "SELECT i.role, i.expires_at, i.accepted_at, o.name, o.slug \
         FROM invites i \
         JOIN organizations o ON o.org_id = i.org_id \
         WHERE i.token_hash = ?1"
    ))
    .bind(&hash)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| InvitePreview {
        org_name: r.get("name"),
        org_slug: r.get("slug"),
        role: r.get("role"),
        expires_at: r.get("expires_at"),
        accepted_at: r.get("accepted_at"),
    }))
}

pub async fn revoke_invite(pool: &DbPool, invite_id: i64, org_id: i64) -> Result<u64> {
    let res = sqlx::query(sql!(
        "DELETE FROM invites WHERE invite_id = ?1 AND org_id = ?2 AND accepted_at IS NULL"
    ))
    .bind(invite_id)
    .bind(org_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

pub async fn member_role(pool: &DbPool, user_id: i64, org_id: i64) -> Result<Option<Role>> {
    let row = sqlx::query(sql!(
        "SELECT role FROM organization_members WHERE user_id = ?1 AND org_id = ?2"
    ))
    .bind(user_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| Role::parse(&r.get::<String, _>("role"))))
}

pub async fn remove_member_guarded(pool: &DbPool, user_id: i64, org_id: i64) -> Result<u64> {
    // Postgres advisory lock serializes concurrent owner removals per org; the COUNT subquery alone is not atomic under READ COMMITTED.
    #[cfg(not(feature = "sqlite"))]
    let affected = {
        let mut tx = pool.begin().await?;
        sqlx::query(sql!("SELECT pg_advisory_xact_lock(?1)"))
            .bind(org_id)
            .execute(&mut *tx)
            .await?;
        let res = sqlx::query(sql!(
            "DELETE FROM organization_members \
             WHERE user_id = ?1 AND org_id = ?2 \
             AND NOT (role = 'owner' AND \
                      (SELECT COUNT(*) FROM organization_members WHERE org_id = ?2 AND role = 'owner') <= 1)"
        ))
        .bind(user_id)
        .bind(org_id)
        .execute(&mut *tx)
        .await?;
        let n = res.rows_affected();
        tx.commit().await?;
        n
    };
    #[cfg(feature = "sqlite")]
    let affected = sqlx::query(sql!(
        "DELETE FROM organization_members \
         WHERE user_id = ?1 AND org_id = ?2 \
         AND NOT (role = 'owner' AND \
                  (SELECT COUNT(*) FROM organization_members WHERE org_id = ?2 AND role = 'owner') <= 1)"
    ))
    .bind(user_id)
    .bind(org_id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected)
}

pub async fn set_member_role_guarded(
    pool: &DbPool,
    user_id: i64,
    org_id: i64,
    role: Role,
) -> Result<u64> {
    if role == Role::Owner {
        let res = sqlx::query(sql!(
            "UPDATE organization_members SET role = 'owner' \
             WHERE user_id = ?1 AND org_id = ?2"
        ))
        .bind(user_id)
        .bind(org_id)
        .execute(pool)
        .await?;
        return Ok(res.rows_affected());
    }
    // Postgres advisory lock serializes concurrent owner demotions per org; the COUNT subquery alone is not atomic under READ COMMITTED.
    #[cfg(not(feature = "sqlite"))]
    let affected = {
        let mut tx = pool.begin().await?;
        sqlx::query(sql!("SELECT pg_advisory_xact_lock(?1)"))
            .bind(org_id)
            .execute(&mut *tx)
            .await?;
        let res = sqlx::query(sql!(
            "UPDATE organization_members SET role = 'member' \
             WHERE user_id = ?1 AND org_id = ?2 \
             AND NOT (role = 'owner' AND \
                      (SELECT COUNT(*) FROM organization_members WHERE org_id = ?2 AND role = 'owner') <= 1)"
        ))
        .bind(user_id)
        .bind(org_id)
        .execute(&mut *tx)
        .await?;
        let n = res.rows_affected();
        tx.commit().await?;
        n
    };
    #[cfg(feature = "sqlite")]
    let affected = sqlx::query(sql!(
        "UPDATE organization_members SET role = 'member' \
         WHERE user_id = ?1 AND org_id = ?2 \
         AND NOT (role = 'owner' AND \
                  (SELECT COUNT(*) FROM organization_members WHERE org_id = ?2 AND role = 'owner') <= 1)"
    ))
    .bind(user_id)
    .bind(org_id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected)
}

pub struct InviteRow {
    pub invite_id: i64,
    pub role: String,
    pub email: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    pub accepted_at: Option<i64>,
}

pub async fn list_org_invites(pool: &DbPool, org_id: i64) -> Result<Vec<InviteRow>> {
    let rows = sqlx::query(sql!(
        "SELECT invite_id, role, email, created_at, expires_at, accepted_at \
         FROM invites WHERE org_id = ?1 ORDER BY created_at DESC"
    ))
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| InviteRow {
            invite_id: r.get("invite_id"),
            role: r.get("role"),
            email: r.get("email"),
            created_at: r.get("created_at"),
            expires_at: r.get("expires_at"),
            accepted_at: r.get("accepted_at"),
        })
        .collect())
}

pub struct OrgMember {
    pub user_id: i64,
    pub role: String,
    pub joined_at: i64,
    pub email: Option<String>,
    pub name: Option<String>,
}

pub async fn list_org_members(pool: &DbPool, org_id: i64) -> Result<Vec<OrgMember>> {
    let rows = sqlx::query(sql!(
        "SELECT m.user_id, m.role, m.joined_at, u.email, u.name \
         FROM organization_members m \
         JOIN users u ON u.user_id = m.user_id \
         WHERE m.org_id = ?1 \
         ORDER BY m.role DESC, u.email"
    ))
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| OrgMember {
            user_id: r.get("user_id"),
            role: r.get("role"),
            joined_at: r.get("joined_at"),
            email: r.get("email"),
            name: r.get("name"),
        })
        .collect())
}

pub struct OrgDetails {
    pub org_id: i64,
    pub slug: String,
    pub name: Option<String>,
    pub is_personal: bool,
    pub ext_iss: Option<String>,
}

pub async fn get_org(pool: &DbPool, org_id: i64) -> Result<Option<OrgDetails>> {
    let row = sqlx::query(sql!(
        "SELECT org_id, slug, name, is_personal, ext_iss FROM organizations WHERE org_id = ?1"
    ))
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| OrgDetails {
        org_id: r.get("org_id"),
        slug: r.get("slug"),
        name: r.get("name"),
        is_personal: r.get::<bool, _>("is_personal"),
        ext_iss: r.get("ext_iss"),
    }))
}

pub async fn count_projects_in_org(pool: &DbPool, org_id: i64) -> Result<i64> {
    let row = sqlx::query(sql!("SELECT COUNT(*) AS c FROM projects WHERE org_id = ?1"))
        .bind(org_id)
        .fetch_one(pool)
        .await?;
    Ok(row.get("c"))
}

pub struct DeleteOrgCounts {
    pub projects: u64,
    pub members: u64,
    pub invites: u64,
    pub integrations: u64,
    pub alert_rules: u64,
    pub digest_schedules: u64,
}

pub enum DeleteOrgOutcome {
    Deleted(DeleteOrgCounts),
    NotDeletable,
}

/// Hard-delete a Native or Forseti org and all it owns; refuses system and personal orgs.
pub async fn delete_org_guarded(pool: &DbPool, org_id: i64) -> Result<DeleteOrgOutcome> {
    let Some(org) = get_org(pool, org_id).await? else {
        return Ok(DeleteOrgOutcome::NotDeletable);
    };
    if org_id == SYSTEM_ORG_ID || org.is_personal {
        return Ok(DeleteOrgOutcome::NotDeletable);
    }

    let mut tx = pool.begin().await?;

    // Per-project cascade (reuses the single source of truth for project-scoped tables).
    let project_rows =
        sqlx::query(sql!("SELECT project_id FROM projects WHERE org_id = ?1"))
            .bind(org_id)
            .fetch_all(&mut *tx)
            .await?;
    let project_ids: Vec<i64> = project_rows.iter().map(|r| r.get("project_id")).collect();
    let projects = project_ids.len() as u64;
    for pid in project_ids {
        crate::queries::projects::delete_project_in_tx(&mut tx, pid).await?;
    }

    // Org-scoped alert_state children first (null-project_id rules the per-project step missed).
    sqlx::query(sql!(
        "DELETE FROM alert_state WHERE alert_rule_id IN (SELECT id FROM alert_rules WHERE org_id = ?1)"
    ))
    .bind(org_id)
    .execute(&mut *tx)
    .await?;

    let alert_rules = sqlx::query(sql!("DELETE FROM alert_rules WHERE org_id = ?1"))
        .bind(org_id).execute(&mut *tx).await?.rows_affected();
    let digest_schedules = sqlx::query(sql!("DELETE FROM digest_schedules WHERE org_id = ?1"))
        .bind(org_id).execute(&mut *tx).await?.rows_affected();
    let integrations = sqlx::query(sql!("DELETE FROM integrations WHERE org_id = ?1"))
        .bind(org_id).execute(&mut *tx).await?.rows_affected();
    let invites = sqlx::query(sql!("DELETE FROM invites WHERE org_id = ?1"))
        .bind(org_id).execute(&mut *tx).await?.rows_affected();
    let members = sqlx::query(sql!("DELETE FROM organization_members WHERE org_id = ?1"))
        .bind(org_id).execute(&mut *tx).await?.rows_affected();

    sqlx::query(sql!("DELETE FROM organizations WHERE org_id = ?1"))
        .bind(org_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(DeleteOrgOutcome::Deleted(DeleteOrgCounts {
        projects,
        members,
        invites,
        integrations,
        alert_rules,
        digest_schedules,
    }))
}

pub struct OrgSummary {
    pub org_id: i64,
    pub slug: String,
    pub name: Option<String>,
}

/// All orgs except the system org, ordered by slug. Used to populate reassign dropdowns.
pub async fn list_non_system_orgs(pool: &DbPool) -> Result<Vec<OrgSummary>> {
    let rows = sqlx::query(sql!(
        "SELECT org_id, slug, name FROM organizations WHERE org_id != ?1 ORDER BY slug"
    ))
    .bind(SYSTEM_ORG_ID)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| OrgSummary {
            org_id: r.get("org_id"),
            slug: r.get("slug"),
            name: r.get("name"),
        })
        .collect())
}

pub struct OrgListItem {
    pub org_id: i64,
    pub slug: String,
    pub name: Option<String>,
    pub is_personal: bool,
    pub ext_iss: Option<String>,
}

/// Every org including the system org, ordered by org_id. Powers the superuser switcher.
pub async fn list_all_orgs(pool: &DbPool) -> Result<Vec<OrgListItem>> {
    let rows = sqlx::query(sql!(
        "SELECT org_id, slug, name, is_personal, ext_iss FROM organizations ORDER BY org_id"
    ))
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| OrgListItem {
            org_id: r.get("org_id"),
            slug: r.get("slug"),
            name: r.get("name"),
            is_personal: r.get::<bool, _>("is_personal"),
            ext_iss: r.get("ext_iss"),
        })
        .collect())
}

/// Lowercase, collapse any run of non-`[a-z0-9]` to a single `-`, trim, fall back to `org`.
/// Constrains the charset so a slug cannot smuggle URL or attribute characters downstream.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "org".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Creates a native (non-personal, non-Forseti) org owned by `created_by`.
/// Slug dedup advances a suffix on collision and never joins an existing org (Invariant 1).
pub async fn create_native_org(
    pool: &DbPool,
    created_by: i64,
    slug: &str,
    name: &str,
) -> Result<i64> {
    #[cfg(feature = "sqlite")]
    let ins = "INSERT OR IGNORE INTO organizations \
               (slug, name, created_by, is_personal, role_sync) \
               VALUES (?1, ?2, ?3, 0, 0)";
    #[cfg(not(feature = "sqlite"))]
    let ins = "INSERT INTO organizations \
               (slug, name, created_by, is_personal, role_sync) \
               VALUES (?1, ?2, ?3, FALSE, FALSE) ON CONFLICT (slug) DO NOTHING";

    let translated = crate::db::translate_sql(ins);

    for attempt in 0u32..100 {
        let candidate = if attempt == 0 {
            slug.to_owned()
        } else {
            format!("{slug}-{attempt}")
        };

        let result = sqlx::query(translated.as_ref())
            .bind(&candidate)
            .bind(name)
            .bind(created_by)
            .execute(pool)
            .await?;

        if result.rows_affected() > 0 {
            // Re-select keys on the just-inserted slug and only on success: never an existing row.
            let row = sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
                .bind(&candidate)
                .fetch_one(pool)
                .await?;
            let org_id: i64 = row.get("org_id");
            add_member(pool, created_by, org_id, Role::Owner).await?;
            return Ok(org_id);
        }
        // rows_affected == 0: slug taken by another org; advance the suffix.
    }

    anyhow::bail!("create_native_org: slug {slug:?} exhausted 100 suffix attempts")
}

/// Outcome of an owner-initiated slug rename. `Taken` means the chosen slug is in use elsewhere.
#[derive(Debug, PartialEq, Eq)]
pub enum RenameOutcome {
    Renamed,
    Taken,
}

/// Renames an org's slug. Rejects system and Forseti-backed orgs and empty input; a uniqueness conflict reports `Taken` and leaves the slug unchanged.
pub async fn rename_org_slug(pool: &DbPool, org_id: i64, requested: &str) -> Result<RenameOutcome> {
    if org_id == SYSTEM_ORG_ID {
        anyhow::bail!("cannot rename the system org");
    }

    // Defense in depth: a Forseti owner could POST directly past the UI gate; its slug would be clobbered on reconcile.
    let row = sqlx::query(sql!("SELECT ext_org_id FROM organizations WHERE org_id = ?1"))
        .bind(org_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("org {org_id} not found"))?;
    if row.get::<Option<String>, _>("ext_org_id").is_some() {
        anyhow::bail!("cannot rename a Forseti-backed org");
    }

    let trimmed = requested.trim();
    if !trimmed.chars().any(|c| c.is_ascii_alphanumeric()) {
        anyhow::bail!("slug cannot be empty");
    }
    let slug = slugify(trimmed);

    let existing = sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
        .bind(&slug)
        .fetch_optional(pool)
        .await?;
    if let Some(row) = existing {
        // A user-chosen slug already in use elsewhere is surfaced, not silently suffixed.
        if row.get::<i64, _>("org_id") != org_id {
            return Ok(RenameOutcome::Taken);
        }
        return Ok(RenameOutcome::Renamed);
    }

    sqlx::query(sql!("UPDATE organizations SET slug = ?1 WHERE org_id = ?2"))
        .bind(&slug)
        .bind(org_id)
        .execute(pool)
        .await?;
    Ok(RenameOutcome::Renamed)
}

/// Counts the user's native orgs (excludes their personal org and Forseti orgs). Enforces the cap.
pub async fn count_user_native_orgs(pool: &DbPool, user_id: i64) -> Result<i64> {
    let row = sqlx::query(sql!(
        "SELECT COUNT(*) AS cnt FROM organizations \
         WHERE created_by = ?1 AND is_personal = ?2 AND ext_org_id IS NULL"
    ))
    .bind(user_id)
    .bind(false)
    .fetch_one(pool)
    .await?;
    Ok(row.get("cnt"))
}

/// Maps to 404 (not 403) to avoid leaking resource existence.
#[derive(Debug)]
pub enum OrgScopeError {
    Denied,
}

impl std::fmt::Display for OrgScopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("project not found or access denied")
    }
}

/// Returns `Ok(())` if `project_id` exists and belongs to `org_id`, else `Err(Denied)`.
pub async fn assert_project_in_org(
    pool: &DbPool,
    project_id: i64,
    org_id: i64,
) -> Result<(), OrgScopeError> {
    let row = sqlx::query(sql!(
        "SELECT org_id FROM projects WHERE project_id = ?1"
    ))
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| OrgScopeError::Denied)?;

    match row {
        Some(r) if r.get::<i64, _>("org_id") == org_id => Ok(()),
        _ => Err(OrgScopeError::Denied),
    }
}

/// Returns the `project_id` that owns `fingerprint`, or `None` if unknown.
pub async fn project_of_fingerprint(pool: &DbPool, fingerprint: &str) -> Result<Option<i64>> {
    let row = sqlx::query(sql!(
        "SELECT project_id FROM issues WHERE fingerprint = ?1"
    ))
    .bind(fingerprint)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.get("project_id")))
}

/// Returns the `project_id` that owns `event_id`, or `None` if unknown.
pub async fn project_of_event(pool: &DbPool, event_id: &str) -> Result<Option<i64>> {
    let row = sqlx::query(sql!(
        "SELECT project_id FROM events WHERE event_id = ?1"
    ))
    .bind(event_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.get("project_id")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ensure_personal_org_is_idempotent() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub", None, None)
            .await
            .unwrap();
        let a = ensure_personal_org(&pool, u.user_id).await.unwrap();
        let b = ensure_personal_org(&pool, u.user_id).await.unwrap();
        assert_eq!(a, b);
        let ms = list_memberships(&pool, u.user_id).await.unwrap();
        assert_eq!(ms.len(), 1);
        assert!(ms[0].is_personal);
        assert_eq!(ms[0].role, "owner");
    }

    async fn personal_slug(pool: &DbPool, user_id: i64) -> String {
        let ms = list_memberships(pool, user_id).await.unwrap();
        ms.into_iter().find(|m| m.is_personal).unwrap().slug
    }

    #[tokio::test]
    async fn personal_org_slug_is_not_sequential_user_id() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-noleak", None, Some("Some One"))
            .await
            .unwrap();
        ensure_personal_org(&pool, u.user_id).await.unwrap();
        let slug = personal_slug(&pool, u.user_id).await;
        let id_str = u.user_id.to_string();
        // Slug must not expose the user_id as a recognizable hyphen-separated segment.
        assert!(!slug.split('-').any(|seg| seg == id_str), "slug leaked user_id as segment: {slug}");
    }

    #[tokio::test]
    async fn personal_org_slug_is_neutral_not_name_derived() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-neutral", None, Some("Some One"))
            .await
            .unwrap();
        ensure_personal_org(&pool, u.user_id).await.unwrap();
        let slug = personal_slug(&pool, u.user_id).await;
        assert!(slug.starts_with("personal-"), "expected neutral slug, got: {slug}");
        assert!(!slug.contains("some"), "slug must not leak the user's name: {slug}");
    }

    // The suffix collision loop shape is exercised by create_native_org_collision_never_joins_existing;
    // the personal slug base is now random per call, so a deterministic personal-specific collision test would need fragile machinery.

    #[tokio::test]
    async fn ensure_personal_org_reslug_is_stable_across_relogin() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-stable", None, Some("Stable One"))
            .await
            .unwrap();
        let a = ensure_personal_org(&pool, u.user_id).await.unwrap();
        rename_org_slug(&pool, a, "chosen-handle").await.unwrap();
        let b = ensure_personal_org(&pool, u.user_id).await.unwrap();
        assert_eq!(a, b, "same org_id across re-login");
        assert_eq!(
            personal_slug(&pool, u.user_id).await,
            "chosen-handle",
            "re-login must not overwrite a renamed personal slug"
        );
    }

    #[tokio::test]
    async fn personal_org_gets_valid_non_userid_slug() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-empty", None, None)
            .await
            .unwrap();
        ensure_personal_org(&pool, u.user_id).await.unwrap();
        let slug = personal_slug(&pool, u.user_id).await;
        assert!(!slug.is_empty());
        assert!(
            slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "slug out of charset: {slug}"
        );
        assert_ne!(slug, format!("user-{}", u.user_id));
    }

    #[tokio::test]
    async fn rename_org_slug_updates_and_slugifies() {
        let pool = crate::db::open_test_pool().await;
        let org_id = insert_native_org(&pool, "old-slug").await;
        assert_eq!(
            rename_org_slug(&pool, org_id, "My New Team").await.unwrap(),
            RenameOutcome::Renamed
        );
        assert_eq!(get_org(&pool, org_id).await.unwrap().unwrap().slug, "my-new-team");
    }

    #[tokio::test]
    async fn rename_org_slug_conflict_reports_taken_and_keeps_slug() {
        let pool = crate::db::open_test_pool().await;
        insert_native_org(&pool, "taken-one").await;
        let org_id = insert_native_org(&pool, "mine").await;
        assert_eq!(
            rename_org_slug(&pool, org_id, "taken-one").await.unwrap(),
            RenameOutcome::Taken
        );
        assert_eq!(get_org(&pool, org_id).await.unwrap().unwrap().slug, "mine");
    }

    #[tokio::test]
    async fn rename_org_slug_rejects_system_org() {
        let pool = crate::db::open_test_pool().await;
        let err = rename_org_slug(&pool, SYSTEM_ORG_ID, "anything").await.unwrap_err();
        assert!(err.to_string().contains("system org"), "got: {err}");
    }

    #[tokio::test]
    async fn rename_org_slug_rejects_empty() {
        let pool = crate::db::open_test_pool().await;
        let org_id = insert_native_org(&pool, "keep-me").await;
        let err = rename_org_slug(&pool, org_id, "   ").await.unwrap_err();
        assert!(err.to_string().contains("empty"), "got: {err}");
        assert_eq!(get_org(&pool, org_id).await.unwrap().unwrap().slug, "keep-me");
    }

    #[tokio::test]
    async fn rename_org_slug_rejects_forseti_org() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-ren-fors", None, None)
            .await
            .unwrap();
        let forseti_id =
            provision_forseti_org(&pool, u.user_id, "https://idp", "org-ren", "fors-slug", "Fors")
                .await
                .unwrap();
        let err = rename_org_slug(&pool, forseti_id, "hijacked").await.unwrap_err();
        assert!(err.to_string().contains("Forseti"), "expected Forseti rejection, got: {err}");
        assert_eq!(get_org(&pool, forseti_id).await.unwrap().unwrap().slug, "fors-slug");
    }

    // The set_org_slug handler gates on is_current_owner; this proves that primitive denies a member and allows the owner.
    #[tokio::test]
    async fn is_current_owner_true_for_owner_false_for_member() {
        let pool = crate::db::open_test_pool().await;
        let owner = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-ico-own", None, None)
            .await
            .unwrap();
        let member = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-ico-mem", None, None)
            .await
            .unwrap();
        let org_id = insert_native_org(&pool, "ico-org").await;
        add_member(&pool, owner.user_id, org_id, Role::Owner).await.unwrap();
        add_member(&pool, member.user_id, org_id, Role::Member).await.unwrap();

        assert!(is_current_owner(&pool, owner.user_id, org_id).await.unwrap());
        assert!(!is_current_owner(&pool, member.user_id, org_id).await.unwrap());
    }

    #[tokio::test]
    async fn org_by_ext_hit_and_miss() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-ext", None, None)
            .await
            .unwrap();
        provision_forseti_org(&pool, u.user_id, "https://iss.example", "org-42", "acme", "Acme")
            .await
            .unwrap();

        assert!(org_by_ext(&pool, "https://iss.example", "org-42")
            .await
            .unwrap()
            .is_some());
        assert!(org_by_ext(&pool, "https://iss.example", "org-99")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn add_member_is_noop_on_existing_role() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-noop", None, None)
            .await
            .unwrap();
        let org_id = ensure_personal_org(&pool, u.user_id).await.unwrap();

        add_member(&pool, u.user_id, org_id, Role::Member)
            .await
            .unwrap();

        let ms = list_memberships(&pool, u.user_id).await.unwrap();
        assert_eq!(ms[0].role, "owner", "add_member must not overwrite existing role");
    }

    #[tokio::test]
    async fn set_member_role_changes_role() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-setrole", None, None)
            .await
            .unwrap();
        let org_id = ensure_personal_org(&pool, u.user_id).await.unwrap();

        set_member_role(&pool, u.user_id, org_id, Role::Member)
            .await
            .unwrap();

        let ms = list_memberships(&pool, u.user_id).await.unwrap();
        assert_eq!(ms[0].role, "member");
    }

    #[tokio::test]
    async fn count_owners_counts_correctly() {
        let pool = crate::db::open_test_pool().await;
        let u1 = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-cnt-a", None, None)
            .await
            .unwrap();
        let u2 = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-cnt-b", None, None)
            .await
            .unwrap();
        let org_id = provision_forseti_org(
            &pool,
            u1.user_id,
            "https://iss.example",
            "org-count",
            "count-test",
            "Count Test",
        )
        .await
        .unwrap();

        add_member(&pool, u2.user_id, org_id, Role::Owner)
            .await
            .unwrap();
        assert_eq!(count_owners(&pool, org_id).await.unwrap(), 2);

        remove_member(&pool, u2.user_id, org_id).await.unwrap();
        assert_eq!(count_owners(&pool, org_id).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn provision_forseti_org_slug_suffix_on_collision() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-slug", None, None)
            .await
            .unwrap();

        provision_forseti_org(
            &pool,
            u.user_id,
            "https://iss.example",
            "org-A",
            "collision",
            "Org A",
        )
        .await
        .unwrap();

        let org2 = provision_forseti_org(
            &pool,
            u.user_id,
            "https://iss.example",
            "org-B",
            "collision",
            "Org B",
        )
        .await
        .unwrap();

        let ms = list_memberships(&pool, u.user_id).await.unwrap();
        let org2_m = ms.iter().find(|m| m.org_id == org2).unwrap();
        assert_eq!(org2_m.slug, "collision-1");
        assert!(!org2_m.is_personal, "provision_forseti_org must not set is_personal");
        // role_sync should be enabled for Forseti-provisioned orgs.
        assert!(role_sync_enabled(&pool, org2).await.unwrap());
    }

    #[test]
    fn slugify_basic_collapse_and_fallback() {
        assert_eq!(slugify("Acme Corp"), "acme-corp");
        assert_eq!(slugify("  Hello,   World!! "), "hello-world");
        assert_eq!(slugify("***"), "org");
        assert_eq!(slugify(""), "org");
    }

    #[test]
    fn slugify_charset_stays_bounded() {
        let s = slugify("Ünïcode 99 __ Café!");
        assert!(
            s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "slug leaked out-of-charset chars: {s:?}"
        );
    }

    #[tokio::test]
    async fn create_native_org_sets_owner_and_non_forseti() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-native", None, None)
            .await
            .unwrap();
        let org_id = create_native_org(&pool, u.user_id, "acme-corp", "Acme Corp")
            .await
            .unwrap();

        let ms = list_memberships(&pool, u.user_id).await.unwrap();
        let m = ms.iter().find(|m| m.org_id == org_id).unwrap();
        assert_eq!(m.role, "owner");
        assert!(!m.is_personal);
        assert!(m.ext_iss.is_none());
        assert_eq!(m.slug, "acme-corp");
        assert!(
            !role_sync_enabled(&pool, org_id).await.unwrap(),
            "native orgs must set role_sync = 0"
        );
    }

    #[tokio::test]
    async fn create_native_org_collision_never_joins_existing() {
        let pool = crate::db::open_test_pool().await;
        let existing = insert_native_org(&pool, "collision").await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-coll", None, None)
            .await
            .unwrap();

        let new_id = create_native_org(&pool, u.user_id, "collision", "Collision")
            .await
            .unwrap();
        assert_ne!(new_id, existing, "must not reuse the existing org");

        let ms = list_memberships(&pool, u.user_id).await.unwrap();
        let m = ms.iter().find(|m| m.org_id == new_id).unwrap();
        assert_eq!(m.slug, "collision-1");

        let row = sqlx::query(sql!(
            "SELECT COUNT(*) AS cnt FROM organization_members WHERE org_id = ?1"
        ))
        .bind(existing)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row.get::<i64, _>("cnt"),
            0,
            "Invariant 1: the pre-existing org must gain no members"
        );
    }

    #[tokio::test]
    async fn count_user_native_orgs_excludes_personal_and_forseti() {
        let pool = crate::db::open_test_pool().await;
        let u =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-count-native", None, None)
                .await
                .unwrap();
        ensure_personal_org(&pool, u.user_id).await.unwrap();
        provision_forseti_org(&pool, u.user_id, "https://idp", "org-x", "fx", "FX")
            .await
            .unwrap();
        create_native_org(&pool, u.user_id, "n1", "N1").await.unwrap();
        create_native_org(&pool, u.user_id, "n2", "N2").await.unwrap();

        assert_eq!(count_user_native_orgs(&pool, u.user_id).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn list_all_orgs_includes_system() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-listall", None, None)
            .await
            .unwrap();
        create_native_org(&pool, u.user_id, "listed", "Listed")
            .await
            .unwrap();

        let all = list_all_orgs(&pool).await.unwrap();
        assert!(
            all.iter().any(|o| o.org_id == SYSTEM_ORG_ID),
            "system org must appear"
        );
        assert!(all.iter().any(|o| o.slug == "listed"));
    }

    // invite tests

    async fn insert_native_org(pool: &DbPool, slug: &str) -> i64 {
        sqlx::query(sql!("INSERT INTO organizations (slug, name) VALUES (?1, 'Test Org')"))
            .bind(slug)
            .execute(pool)
            .await
            .unwrap();
        let row = sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap();
        row.get("org_id")
    }

    #[tokio::test]
    async fn create_invite_on_system_org_rejected() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-inv-sys", None, None)
            .await
            .unwrap();
        let err = create_invite(&pool, SYSTEM_ORG_ID, Role::Member, None, u.user_id, 3600)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("system org"),
            "expected system-org rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn create_invite_on_forseti_org_rejected() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-inv-fors", None, None)
            .await
            .unwrap();
        let forseti_id =
            provision_forseti_org(&pool, u.user_id, "https://idp", "org-fors", "fors", "Fors")
                .await
                .unwrap();
        let err = create_invite(&pool, forseti_id, Role::Member, None, u.user_id, 3600)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("Forseti"),
            "expected Forseti-org rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn accept_invite_adds_membership_with_role() {
        let pool = crate::db::open_test_pool().await;
        let owner =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-inv-own", None, None)
                .await
                .unwrap();
        let invitee =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-inv-mem", None, None)
                .await
                .unwrap();
        let org_id = insert_native_org(&pool, "native-accept").await;
        add_member(&pool, owner.user_id, org_id, Role::Owner)
            .await
            .unwrap();

        let token = create_invite(&pool, org_id, Role::Member, None, owner.user_id, 3600)
            .await
            .unwrap();

        let returned_org = accept_invite(&pool, &token, invitee.user_id)
            .await
            .unwrap();
        assert_eq!(returned_org, org_id);

        let ms = list_memberships(&pool, invitee.user_id).await.unwrap();
        let m = ms.iter().find(|m| m.org_id == org_id).unwrap();
        assert_eq!(m.role, "member");
    }

    #[tokio::test]
    async fn accept_invite_expired_token_rejected() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-inv-exp", None, None)
            .await
            .unwrap();
        let u2 =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-inv-exp2", None, None)
                .await
                .unwrap();
        let org_id = insert_native_org(&pool, "native-exp").await;

        let token = create_invite(&pool, org_id, Role::Member, None, u.user_id, -3600)
            .await
            .unwrap();

        let err = accept_invite(&pool, &token, u2.user_id).await.unwrap_err();
        assert!(
            err.to_string().contains("expired"),
            "expected expiry rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn accept_invite_already_accepted_rejected() {
        let pool = crate::db::open_test_pool().await;
        let owner =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-inv-aa-own", None, None)
                .await
                .unwrap();
        let u2 =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-inv-aa-u2", None, None)
                .await
                .unwrap();
        let u3 =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-inv-aa-u3", None, None)
                .await
                .unwrap();
        let org_id = insert_native_org(&pool, "native-aa").await;

        let token = create_invite(&pool, org_id, Role::Member, None, owner.user_id, 3600)
            .await
            .unwrap();

        accept_invite(&pool, &token, u2.user_id).await.unwrap();

        let err = accept_invite(&pool, &token, u3.user_id).await.unwrap_err();
        assert!(
            err.to_string().contains("already accepted"),
            "expected already-accepted rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn system_org_has_no_members_after_invite_flows() {
        let pool = crate::db::open_test_pool().await;
        let owner =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-iso-own", None, None)
                .await
                .unwrap();
        let invitee =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-iso-inv", None, None)
                .await
                .unwrap();

        let native_id = insert_native_org(&pool, "native-iso").await;
        let token = create_invite(&pool, native_id, Role::Member, None, owner.user_id, 3600)
            .await
            .unwrap();
        accept_invite(&pool, &token, invitee.user_id)
            .await
            .unwrap();

        let row = sqlx::query(sql!(
            "SELECT COUNT(*) AS cnt FROM organization_members WHERE org_id = ?1"
        ))
        .bind(SYSTEM_ORG_ID)
        .fetch_one(&pool)
        .await
        .unwrap();
        let cnt: i64 = row.get("cnt");
        assert_eq!(cnt, 0, "org 1 must have zero organization_members rows");
    }

    // end invite tests

    #[tokio::test]
    async fn revoke_invite_scoped_and_pending_only() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-rev-own", None, None)
            .await
            .unwrap();
        let acceptor =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-rev-acc", None, None)
                .await
                .unwrap();
        let org_a = insert_native_org(&pool, "rev-org-a").await;
        let org_b = insert_native_org(&pool, "rev-org-b").await;

        // Create a pending invite and accept it
        let accepted_tok = create_invite(&pool, org_a, Role::Member, None, u.user_id, 3600)
            .await
            .unwrap();
        let accepted_id: i64 = sqlx::query(sql!(
            "SELECT invite_id FROM invites WHERE org_id = ?1 ORDER BY invite_id DESC"
        ))
        .bind(org_a)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("invite_id");
        accept_invite(&pool, &accepted_tok, acceptor.user_id).await.unwrap();

        // Create a second pending invite
        let _pending_tok = create_invite(&pool, org_a, Role::Owner, None, u.user_id, 3600)
            .await
            .unwrap();
        let pending_id: i64 = sqlx::query(sql!(
            "SELECT invite_id FROM invites WHERE org_id = ?1 AND accepted_at IS NULL ORDER BY invite_id DESC"
        ))
        .bind(org_a)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("invite_id");

        // wrong org returns 0 and invite still present
        let affected = revoke_invite(&pool, pending_id, org_b).await.unwrap();
        assert_eq!(affected, 0);
        assert!(list_org_invites(&pool, org_a).await.unwrap().iter().any(|r| r.invite_id == pending_id));

        // accepted invite returns 0
        let affected = revoke_invite(&pool, accepted_id, org_a).await.unwrap();
        assert_eq!(affected, 0);

        // right org + pending returns 1
        let affected = revoke_invite(&pool, pending_id, org_a).await.unwrap();
        assert_eq!(affected, 1);
        assert!(!list_org_invites(&pool, org_a).await.unwrap().iter().any(|r| r.invite_id == pending_id));
    }

    #[tokio::test]
    async fn member_role_some_and_none() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-mr-own", None, None)
            .await
            .unwrap();
        let nonmember =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-mr-non", None, None)
                .await
                .unwrap();
        let org_id = insert_native_org(&pool, "mr-org").await;
        add_member(&pool, u.user_id, org_id, Role::Owner).await.unwrap();

        let role = member_role(&pool, u.user_id, org_id).await.unwrap();
        assert_eq!(role, Some(Role::Owner));

        let none = member_role(&pool, nonmember.user_id, org_id).await.unwrap();
        assert_eq!(none, None);
    }

    #[tokio::test]
    async fn remove_member_guarded_blocks_sole_owner() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-rmg-sole", None, None)
            .await
            .unwrap();
        let org_id = insert_native_org(&pool, "rmg-sole-org").await;
        add_member(&pool, u.user_id, org_id, Role::Owner).await.unwrap();

        // sole owner: returns 0, stays
        let affected = remove_member_guarded(&pool, u.user_id, org_id).await.unwrap();
        assert_eq!(affected, 0);
        assert_eq!(count_owners(&pool, org_id).await.unwrap(), 1);

        // non-owner member: returns 1
        let m = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-rmg-mem", None, None)
            .await
            .unwrap();
        add_member(&pool, m.user_id, org_id, Role::Member).await.unwrap();
        let affected = remove_member_guarded(&pool, m.user_id, org_id).await.unwrap();
        assert_eq!(affected, 1);
    }

    #[tokio::test]
    // True cross-transaction concurrency is not unit-testable in the SQLite test env; this covers sequential guard logic only.
    async fn remove_member_guarded_blocks_second_owner_removal() {
        let pool = crate::db::open_test_pool().await;
        let u1 = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-rmg-o1", None, None)
            .await
            .unwrap();
        let u2 = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-rmg-o2", None, None)
            .await
            .unwrap();
        let org_id = insert_native_org(&pool, "rmg-two-org").await;
        add_member(&pool, u1.user_id, org_id, Role::Owner).await.unwrap();
        add_member(&pool, u2.user_id, org_id, Role::Owner).await.unwrap();

        // remove u1: succeeds (u2 still owner)
        let a1 = remove_member_guarded(&pool, u1.user_id, org_id).await.unwrap();
        assert_eq!(a1, 1);
        // remove u2: sole owner, must be blocked
        let a2 = remove_member_guarded(&pool, u2.user_id, org_id).await.unwrap();
        assert_eq!(a2, 0);
        assert!(count_owners(&pool, org_id).await.unwrap() >= 1);
    }

    #[tokio::test]
    async fn set_member_role_guarded_blocks_sole_owner_demote() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-smrg-sole", None, None)
            .await
            .unwrap();
        let co = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-smrg-co", None, None)
            .await
            .unwrap();
        let org_id = insert_native_org(&pool, "smrg-org").await;
        add_member(&pool, u.user_id, org_id, Role::Owner).await.unwrap();

        // demote sole owner: returns 0
        let affected = set_member_role_guarded(&pool, u.user_id, org_id, Role::Member)
            .await
            .unwrap();
        assert_eq!(affected, 0);
        assert_eq!(
            member_role(&pool, u.user_id, org_id).await.unwrap(),
            Some(Role::Owner)
        );

        // add co-owner, demote u: returns 1
        add_member(&pool, co.user_id, org_id, Role::Owner).await.unwrap();
        let affected = set_member_role_guarded(&pool, u.user_id, org_id, Role::Member)
            .await
            .unwrap();
        assert_eq!(affected, 1);
        assert_eq!(
            member_role(&pool, u.user_id, org_id).await.unwrap(),
            Some(Role::Member)
        );

        // promote always applies
        let affected = set_member_role_guarded(&pool, u.user_id, org_id, Role::Owner)
            .await
            .unwrap();
        assert_eq!(affected, 1);
        assert_eq!(
            member_role(&pool, u.user_id, org_id).await.unwrap(),
            Some(Role::Owner)
        );
    }

    #[tokio::test]
    async fn list_org_invites_returns_correct_rows() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-loi-own", None, None)
            .await
            .unwrap();
        let acceptor =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-loi-acc", None, None)
                .await
                .unwrap();
        let org_a = insert_native_org(&pool, "loi-org-a").await;
        let org_b = insert_native_org(&pool, "loi-org-b").await;

        let tok1 = create_invite(&pool, org_a, Role::Owner, Some("a@x.com"), u.user_id, 3600)
            .await
            .unwrap();
        let _tok2 = create_invite(&pool, org_a, Role::Member, Some("b@x.com"), u.user_id, 3600)
            .await
            .unwrap();
        // create an invite for org_b that must not appear in org_a results
        let _tok3 = create_invite(&pool, org_b, Role::Member, None, u.user_id, 3600)
            .await
            .unwrap();

        accept_invite(&pool, &tok1, acceptor.user_id).await.unwrap();

        let rows = list_org_invites(&pool, org_a).await.unwrap();
        assert_eq!(rows.len(), 2, "only org_a invites");
        let accepted = rows.iter().find(|r| r.email.as_deref() == Some("a@x.com")).unwrap();
        assert!(accepted.accepted_at.is_some());
        let pending = rows.iter().find(|r| r.email.as_deref() == Some("b@x.com")).unwrap();
        assert!(pending.accepted_at.is_none());
    }

    #[tokio::test]
    async fn list_org_members_returns_members_with_roles() {
        let pool = crate::db::open_test_pool().await;
        let owner =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-lom-own", None, None)
                .await
                .unwrap();
        let member =
            crate::queries::users::upsert_from_oidc(&pool, "iss", "sub-lom-mem", Some("mem@example.com"), None)
                .await
                .unwrap();
        let org_id = insert_native_org(&pool, "lom-org").await;
        add_member(&pool, owner.user_id, org_id, Role::Owner).await.unwrap();
        add_member(&pool, member.user_id, org_id, Role::Member).await.unwrap();

        let members = list_org_members(&pool, org_id).await.unwrap();
        assert_eq!(members.len(), 2);
        let owner_row = members.iter().find(|m| m.user_id == owner.user_id).unwrap();
        assert_eq!(owner_row.role, "owner");
        let member_row = members.iter().find(|m| m.user_id == member.user_id).unwrap();
        assert_eq!(member_row.role, "member");
        assert_eq!(member_row.email.as_deref(), Some("mem@example.com"));
    }

    #[tokio::test]
    async fn get_org_hit_and_miss() {
        let pool = crate::db::open_test_pool().await;
        let org_id = insert_native_org(&pool, "get-org-test").await;

        let found = get_org(&pool, org_id).await.unwrap();
        assert!(found.is_some());
        let details = found.unwrap();
        assert_eq!(details.slug, "get-org-test");
        assert!(!details.is_personal);

        let missing = get_org(&pool, 999_999).await.unwrap();
        assert!(missing.is_none());
    }

    async fn insert_test_project(pool: &DbPool, project_id: i64, org_id: i64) {
        sqlx::query(sql!(
            "INSERT INTO projects (project_id, org_id) VALUES (?1, ?2)"
        ))
        .bind(project_id)
        .bind(org_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn assert_project_in_org_ok_and_denied() {
        let pool = crate::db::open_test_pool().await;
        let org_a = insert_native_org(&pool, "guard-org-a").await;
        let org_b = insert_native_org(&pool, "guard-org-b").await;
        insert_test_project(&pool, 100, org_a).await;

        assert!(assert_project_in_org(&pool, 100, org_a).await.is_ok());
        assert!(assert_project_in_org(&pool, 100, org_b).await.is_err());
        assert!(assert_project_in_org(&pool, 999, org_a).await.is_err());
    }

    #[tokio::test]
    async fn cross_org_fingerprint_guard_rejects_mismatched_project_and_foreign_org() {
        use crate::queries::test_helpers::insert_test_issue;
        let pool = crate::db::open_test_pool().await;
        let org_a = insert_native_org(&pool, "fp-guard-org-a").await;
        let org_b = insert_native_org(&pool, "fp-guard-org-b").await;
        insert_test_project(&pool, 401, org_a).await;
        insert_test_project(&pool, 402, org_b).await;
        insert_test_issue(&pool, "fp-org-a", 401, None, None, 0, 0, 0, "unresolved").await;
        insert_test_issue(&pool, "fp-org-b", 402, None, None, 0, 0, 0, "unresolved").await;

        // fp-org-b belongs to project 402, not 401: mismatch detected
        let fp_project = project_of_fingerprint(&pool, "fp-org-b").await.unwrap();
        assert_eq!(fp_project, Some(402));
        assert_ne!(fp_project.unwrap(), 401, "cross-project fingerprint must be detected");

        // fp-org-a is in org_a; org_b caller must be denied
        assert!(assert_project_in_org(&pool, 401, org_a).await.is_ok());
        assert!(assert_project_in_org(&pool, 401, org_b).await.is_err());
    }

    #[tokio::test]
    async fn project_of_fingerprint_hit_and_miss() {
        use crate::queries::test_helpers::insert_test_issue;
        let pool = crate::db::open_test_pool().await;
        insert_test_project(&pool, 200, 1).await;
        insert_test_issue(&pool, "fp-abc", 200, None, None, 0, 0, 0, "unresolved").await;

        let found = project_of_fingerprint(&pool, "fp-abc").await.unwrap();
        assert_eq!(found, Some(200));

        let missing = project_of_fingerprint(&pool, "fp-unknown").await.unwrap();
        assert_eq!(missing, None);
    }

    #[tokio::test]
    async fn project_of_event_hit_and_miss() {
        use crate::queries::test_helpers::insert_test_event;
        let pool = crate::db::open_test_pool().await;
        insert_test_project(&pool, 300, 1).await;
        insert_test_event(&pool, "evt-xyz", 300, 0, None, None, None).await;

        let found = project_of_event(&pool, "evt-xyz").await.unwrap();
        assert_eq!(found, Some(300));

        let missing = project_of_event(&pool, "evt-unknown").await.unwrap();
        assert_eq!(missing, None);
    }

    #[tokio::test]
    async fn forseti_org_is_not_personal() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "sub", None, None)
            .await
            .unwrap();
        let org = provision_forseti_org(&pool, u.user_id, "https://idp", "acme", "acme", "Acme")
            .await
            .unwrap();
        let row = sqlx::query(sql!("SELECT is_personal, ext_org_id FROM organizations WHERE org_id = ?1"))
            .bind(org)
            .fetch_one(&pool)
            .await
            .unwrap();
        let is_personal: bool = row.get("is_personal");
        let ext: Option<String> = row.get("ext_org_id");
        assert!(!is_personal);
        assert_eq!(ext.as_deref(), Some("acme"));
    }

    #[tokio::test]
    async fn delete_org_guarded_removes_native_org_and_projects() {
        use sqlx::Row;
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "del-1", None, None)
            .await
            .unwrap();
        let org = create_native_org(&pool, u.user_id, "to-delete", "To Delete").await.unwrap();

        // A project in the org plus an event and a project-scoped alert rule + state.
        sqlx::query(sql!("INSERT INTO projects (project_id, status, source, org_id) VALUES (7100, 'active', 'manual', ?1)"))
            .bind(org).execute(&pool).await.unwrap();
        sqlx::query(sql!("INSERT INTO alert_rules (org_id, project_id, trigger_kind) VALUES (?1, 7100, 'rate')"))
            .bind(org).execute(&pool).await.unwrap();

        let outcome = delete_org_guarded(&pool, org).await.unwrap();
        assert!(matches!(outcome, DeleteOrgOutcome::Deleted(_)));

        let org_rows: i64 = sqlx::query(sql!("SELECT COUNT(*) AS c FROM organizations WHERE org_id = ?1"))
            .bind(org).fetch_one(&pool).await.unwrap().get("c");
        assert_eq!(org_rows, 0, "org row gone");
        let proj_rows: i64 = sqlx::query(sql!("SELECT COUNT(*) AS c FROM projects WHERE org_id = ?1"))
            .bind(org).fetch_one(&pool).await.unwrap().get("c");
        assert_eq!(proj_rows, 0, "projects gone");
    }

    #[tokio::test]
    async fn delete_org_guarded_removes_org_scoped_alert_state_orphans() {
        use sqlx::Row;
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "del-2", None, None)
            .await
            .unwrap();
        let org = create_native_org(&pool, u.user_id, "orphan-org", "Orphan").await.unwrap();

        // Alert rule with NULL project_id (purely org-scoped) plus its alert_state child.
        sqlx::query(sql!("INSERT INTO alert_rules (org_id, project_id, trigger_kind) VALUES (?1, NULL, 'rate')"))
            .bind(org).execute(&pool).await.unwrap();
        let rule_id: i64 = sqlx::query(sql!("SELECT id FROM alert_rules WHERE org_id = ?1"))
            .bind(org).fetch_one(&pool).await.unwrap().get("id");
        sqlx::query(sql!("INSERT INTO alert_state (alert_rule_id, fingerprint) VALUES (?1, 'test-fp')"))
            .bind(rule_id).execute(&pool).await.unwrap();

        delete_org_guarded(&pool, org).await.unwrap();

        let state_rows: i64 = sqlx::query(sql!("SELECT COUNT(*) AS c FROM alert_state WHERE alert_rule_id = ?1"))
            .bind(rule_id).fetch_one(&pool).await.unwrap().get("c");
        assert_eq!(state_rows, 0, "org-scoped alert_state must not orphan");
    }

    #[tokio::test]
    async fn delete_org_guarded_refuses_system_and_personal() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "del-3", None, None)
            .await
            .unwrap();
        let personal = ensure_personal_org(&pool, u.user_id).await.unwrap();

        assert!(matches!(delete_org_guarded(&pool, crate::orgs::SYSTEM_ORG_ID).await.unwrap(), DeleteOrgOutcome::NotDeletable));
        assert!(matches!(delete_org_guarded(&pool, personal).await.unwrap(), DeleteOrgOutcome::NotDeletable));
    }

    #[tokio::test]
    async fn delete_org_guarded_does_not_touch_other_orgs() {
        use sqlx::Row;
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "del-4", None, None)
            .await
            .unwrap();
        let keep = create_native_org(&pool, u.user_id, "keep", "Keep").await.unwrap();
        let drop = create_native_org(&pool, u.user_id, "drop", "Drop").await.unwrap();
        sqlx::query(sql!("INSERT INTO projects (project_id, status, source, org_id) VALUES (7200, 'active', 'manual', ?1)"))
            .bind(keep).execute(&pool).await.unwrap();

        delete_org_guarded(&pool, drop).await.unwrap();

        let kept: i64 = sqlx::query(sql!("SELECT COUNT(*) AS c FROM projects WHERE org_id = ?1"))
            .bind(keep).fetch_one(&pool).await.unwrap().get("c");
        assert_eq!(kept, 1, "other org's project survives");
    }

    #[tokio::test]
    async fn count_projects_in_org_counts_only_that_org() {
        let pool = crate::db::open_test_pool().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "iss", "cnt-1", None, None)
            .await
            .unwrap();
        let org = create_native_org(&pool, u.user_id, "counted", "Counted").await.unwrap();
        sqlx::query(sql!("INSERT INTO projects (project_id, status, source, org_id) VALUES (7300, 'active', 'manual', ?1)"))
            .bind(org).execute(&pool).await.unwrap();
        sqlx::query(sql!("INSERT INTO projects (project_id, status, source, org_id) VALUES (7301, 'active', 'manual', ?1)"))
            .bind(org).execute(&pool).await.unwrap();
        assert_eq!(count_projects_in_org(&pool, org).await.unwrap(), 2);
    }

    /// Fails if a new `org_id`-bearing table is added without being wired into
    /// `delete_org_guarded`, which would orphan its rows on org deletion.
    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn delete_org_covers_all_org_scoped_tables() {
        use sqlx::Row;
        let pool = crate::db::open_test_pool().await;
        let rows = sqlx::query(
            "SELECT DISTINCT m.name FROM sqlite_master m, pragma_table_info(m.name) p \
             WHERE m.type='table' AND p.name='org_id'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        // Tables deleted directly by org_id in delete_org_guarded.
        const ORG_SCOPED_TABLES: &[&str] = &[
            "organization_members",
            "invites",
            "alert_rules",
            "digest_schedules",
            "integrations",
        ];

        for row in &rows {
            let table: String = row.get(0);
            // `organizations` is the root row; `projects` is covered by the per-project cascade.
            assert!(
                table == "organizations"
                    || table == "projects"
                    || ORG_SCOPED_TABLES.contains(&table.as_str()),
                "table `{table}` has an org_id column but is not covered by delete_org_guarded; \
                 add it (and a delete case) or it will orphan rows"
            );
        }
    }
}
