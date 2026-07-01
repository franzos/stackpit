use askama::Template;
use axum::extract::{Extension, Form, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use serde::Deserialize;
use stackpit_auth::AuthContext;

use crate::db::DbPool;
use crate::html::{filters, html_error, render_template};
use crate::html::utils::Csrf;
use crate::orgs::extractor::{pack, ActiveOrg, ACTIVE_ORG_COOKIE};
use crate::orgs::{OrgKind, Role, SYSTEM_ORG_ID};
use crate::queries::orgs::{
    accept_invite, count_owners, count_projects_in_org, count_user_native_orgs, create_invite,
    create_native_org, delete_org_guarded, get_invite_preview, get_org, is_current_owner,
    list_all_orgs, list_memberships, list_org_invites, list_org_members, member_role,
    remove_member_guarded, rename_org_slug, revoke_invite, set_member_role_guarded, slugify,
    DeleteOrgOutcome, RenameOutcome,
};
use crate::queries::users;
use crate::server::AppState;

#[derive(Deserialize)]
pub struct SwitchOrgForm {
    org_id: i64,
}

/// Superusers may switch anywhere; members must be listed and cannot target SYSTEM_ORG_ID.
pub fn can_switch_to(target: i64, memberships: &[i64], is_superuser: bool) -> bool {
    if target == SYSTEM_ORG_ID && !is_superuser {
        return false;
    }
    if is_superuser {
        return true;
    }
    memberships.contains(&target)
}

/// Display-only; real enforcement is the atomic SQL in remove_member_guarded/set_member_role_guarded.
pub fn strands_org(target_is_owner: bool, owner_count: i64) -> bool {
    target_is_owner && owner_count <= 1
}

pub async fn switch_org(
    State(state): State<AppState>,
    active_org: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Form(form): Form<SwitchOrgForm>,
) -> impl IntoResponse {
    let is_superuser = active_org.role.is_none();

    let member_ids: Vec<i64> = if is_superuser {
        vec![]
    } else {
        let (iss, sub) = match opt_auth.as_ref().map(|e| &e.0) {
            Some(AuthContext::User { iss, sub, .. }) => (iss.as_str(), sub.as_str()),
            _ => return StatusCode::FORBIDDEN.into_response(),
        };
        let user = match users::find_by_iss_sub(&state.pool, iss, sub).await {
            Ok(Some(u)) => u,
            Ok(None) => return StatusCode::FORBIDDEN.into_response(),
            Err(e) => {
                tracing::error!("find_by_iss_sub failed in org switch: {e:#}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
        match list_memberships(&state.pool, user.user_id).await {
            Ok(ms) => ms.iter().map(|m| m.org_id).collect(),
            Err(e) => {
                tracing::error!("list_memberships failed in org switch: {e:#}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    };

    if !can_switch_to(form.org_id, &member_ids, is_superuser) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(enc) = state.encryptor.as_deref() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Some(packed) = pack(enc, form.org_id) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let secure = state.config.server.cookies_should_be_secure();
    let secure_flag = if secure { "; Secure" } else { "" };
    let cookie =
        format!("{ACTIVE_ORG_COOKIE}={packed}; Path=/; SameSite=Strict; HttpOnly{secure_flag}");

    let mut resp = axum::response::Redirect::to("/web/").into_response();
    if let Ok(val) = cookie.parse() {
        resp.headers_mut().append("set-cookie", val);
    }
    resp
}

#[derive(Deserialize)]
pub struct CreateInviteForm {
    role: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default, deserialize_with = "crate::html::utils::empty_string_as_none")]
    ttl_secs: Option<i64>,
}

#[derive(Template)]
#[template(path = "invite_accept.html")]
struct InviteAcceptTemplate {
    token: String,
    org_name: String,
    role: String,
    csrf_token: String,
    error: Option<String>,
}

/// `POST /web/organizations/:org_id/invites`: create an invite for a native org.
/// Requires the caller to be an owner of the PATH org (not the active org).
pub async fn create_org_invite(
    State(state): State<AppState>,
    active_org: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Path(path_org_id): Path<i64>,
    Form(form): Form<CreateInviteForm>,
) -> impl IntoResponse {
    let is_superuser = active_org.role.is_none();

    let (iss, sub) = match opt_auth.as_ref().map(|e| &e.0) {
        Some(AuthContext::User { iss, sub, .. }) => (iss.as_str(), sub.as_str()),
        _ => return StatusCode::FORBIDDEN.into_response(),
    };

    let user = match users::find_by_iss_sub(&state.pool, iss, sub).await {
        Ok(Some(u)) => u,
        Ok(None) => return StatusCode::FORBIDDEN.into_response(),
        Err(e) => {
            tracing::error!("find_by_iss_sub failed in create_org_invite: {e:#}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Verify caller owns the PATH org (not the active org; IDOR prevention).
    if !is_superuser {
        match is_current_owner(&state.pool, user.user_id, path_org_id).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(e) => {
                tracing::error!("is_current_owner check failed: {e:#}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    let role = Role::parse(&form.role);
    let email = form.email.as_deref().filter(|s| !s.is_empty());
    let ttl_secs = form.ttl_secs.unwrap_or(7 * 24 * 3600);

    let token = match create_invite(&state.pool, path_org_id, role, email, user.user_id, ttl_secs)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("create_invite failed: {e:#}");
            return html_error(StatusCode::FORBIDDEN, &e.to_string());
        }
    };

    let base = state
        .config
        .server
        .external_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let invite_url = format!("{base}/web/invite/{token}");
    let escaped = crate::util::encoding::escape_html(&invite_url);

    let ttl_label = if ttl_secs % 86400 == 0 {
        format!("{} day(s)", ttl_secs / 86400)
    } else if ttl_secs % 3600 == 0 {
        format!("{} hour(s)", ttl_secs / 3600)
    } else {
        format!("{} seconds", ttl_secs)
    };

    let body = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="light dark"><title>Invite created - Stackpit</title>
<link rel="preload" href="/web/_assets/fonts/Inter-Regular.woff2" as="font" type="font/woff2" crossorigin>
<link rel="preload" href="/web/_assets/fonts/Inter-SemiBold.woff2" as="font" type="font/woff2" crossorigin>
<link rel="stylesheet" href="/web/_assets/style.css">
<link rel="icon" type="image/svg+xml" href="/web/_assets/icon.svg"></head>
<body class="min-h-screen flex items-center justify-center px-4">
<div class="w-full max-w-md">
<div class="flex flex-col items-center mb-6">
<div class="flex items-center gap-2 mb-4"><img src="/web/_assets/icon.svg" alt="" width="28" height="28"><span class="text-xl font-semibold tracking-tight">Stackpit</span></div>
<h1 class="page-h1">Invite created</h1>
</div>
<div class="card card-pad space-y-4">
<p class="text-[13px] text-muted">Share this link. It is valid for {ttl_label} and single-use.</p>
<pre class="codeblock select-all">{}</pre>
<a href="/web/organizations/{path_org_id}/members" class="btn btn-secondary w-full justify-center h-10">Back to members</a>
</div>
</div>
</body></html>"#,
        escaped
    );
    axum::response::Html(body).into_response()
}

#[derive(Deserialize)]
pub struct RenameSlugForm {
    slug: String,
}

/// `POST /web/organizations/:org_id/slug`: rename the org's slug (owner of the PATH org, incl. personal).
/// Superusers bypass the owner check; non-owners get 403 (IDOR-safe against the path org).
pub async fn set_org_slug(
    State(state): State<AppState>,
    active_org: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Path(path_org_id): Path<i64>,
    Form(form): Form<RenameSlugForm>,
) -> impl IntoResponse {
    let is_superuser = active_org.role.is_none();

    let (iss, sub) = match opt_auth.as_ref().map(|e| &e.0) {
        Some(AuthContext::User { iss, sub, .. }) => (iss.as_str(), sub.as_str()),
        _ => return StatusCode::FORBIDDEN.into_response(),
    };

    let user = match users::find_by_iss_sub(&state.pool, iss, sub).await {
        Ok(Some(u)) => u,
        Ok(None) => return StatusCode::FORBIDDEN.into_response(),
        Err(e) => {
            tracing::error!("find_by_iss_sub failed in set_org_slug: {e:#}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Verify caller owns the PATH org (not the active org; IDOR prevention).
    if !is_superuser {
        match is_current_owner(&state.pool, user.user_id, path_org_id).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(e) => {
                tracing::error!("is_current_owner check failed in set_org_slug: {e:#}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    match rename_org_slug(&state.pool, path_org_id, &form.slug).await {
        Ok(RenameOutcome::Renamed) => {
            Redirect::to(&format!("/web/organizations/{path_org_id}/members")).into_response()
        }
        Ok(RenameOutcome::Taken) => html_error(StatusCode::CONFLICT, "That slug is already taken."),
        Err(e) => html_error(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

/// `GET /web/invite/:token`: show the invite accept page.
pub async fn get_invite_accept(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Csrf(csrf): Csrf,
) -> impl IntoResponse {
    let now = chrono::Utc::now().timestamp();

    match get_invite_preview(&state.pool, &token).await {
        Ok(Some(preview)) => {
            let error = if preview.accepted_at.is_some() {
                Some("This invite has already been accepted.".to_string())
            } else if now > preview.expires_at {
                Some("This invite has expired.".to_string())
            } else {
                None
            };
            let org_name = preview.org_name.unwrap_or(preview.org_slug);
            render_template(&InviteAcceptTemplate {
                token,
                org_name,
                role: preview.role,
                csrf_token: csrf,
                error,
            })
        }
        Ok(None) => html_error(StatusCode::NOT_FOUND, "Invite not found or invalid."),
        Err(e) => {
            tracing::error!("get_invite_preview failed: {e:#}");
            html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to look up invite.")
        }
    }
}

/// `POST /web/invite/:token`: accept the invite for the authenticated user.
pub async fn post_invite_accept(
    State(state): State<AppState>,
    opt_auth: Option<Extension<AuthContext>>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let (iss, sub) = match opt_auth.as_ref().map(|e| &e.0) {
        Some(AuthContext::User { iss, sub, .. }) => (iss.as_str(), sub.as_str()),
        _ => return StatusCode::FORBIDDEN.into_response(),
    };

    let user = match users::find_by_iss_sub(&state.pool, iss, sub).await {
        Ok(Some(u)) => u,
        Ok(None) => return StatusCode::FORBIDDEN.into_response(),
        Err(e) => {
            tracing::error!("find_by_iss_sub failed in post_invite_accept: {e:#}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    match accept_invite(&state.pool, &token, user.user_id).await {
        Ok(_) => Redirect::to("/web/").into_response(),
        Err(e) => html_error(StatusCode::FORBIDDEN, &e.to_string()),
    }
}

struct MemberView {
    user_id: i64,
    name: String,
    email: Option<String>,
    role: String,
    joined_at: i64,
    can_remove: bool,
    can_demote: bool,
}

struct InviteView {
    invite_id: i64,
    role: String,
    email: Option<String>,
    created_at: i64,
    expires_at: i64,
    status: String,
}

#[derive(Template)]
#[template(path = "org_members.html")]
struct OrgMembersTemplate {
    org_id: i64,
    org_name: String,
    kind: String,
    is_native: bool,
    can_manage: bool,
    can_rename: bool,
    current_slug: String,
    members: Vec<MemberView>,
    invites: Vec<InviteView>,
    csrf_token: String,
    can_delete: bool,
    project_count: i64,
    member_count: i64,
    slug: String,
}

struct OrgRow {
    org_id: i64,
    name: String,
    slug: String,
    kind: String,
    role: Option<String>,
    active: bool,
}

#[derive(Template)]
#[template(path = "orgs.html")]
struct OrgsTemplate {
    rows: Vec<OrgRow>,
    show_create: bool,
    csrf_token: String,
}

/// `GET /web/organizations`: list the caller's orgs (or all orgs for a superuser) and offer creation.
pub async fn orgs_index(
    State(state): State<AppState>,
    active_org: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Csrf(csrf): Csrf,
) -> axum::response::Response {
    let is_superuser = active_org.role.is_none();

    if is_superuser {
        let orgs = match list_all_orgs(&state.pool).await {
            Ok(o) => o,
            Err(e) => {
                tracing::error!("list_all_orgs failed: {e:#}");
                return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load orgs.");
            }
        };
        let rows = orgs
            .into_iter()
            .map(|o| {
                let kind = OrgKind::classify(o.org_id, o.is_personal, o.ext_iss.is_some());
                OrgRow {
                    org_id: o.org_id,
                    name: o.name.unwrap_or_else(|| o.slug.clone()),
                    slug: o.slug,
                    kind: kind.label().to_owned(),
                    role: None,
                    active: o.org_id == active_org.org_id,
                }
            })
            .collect();
        return render_template(&OrgsTemplate { rows, show_create: false, csrf_token: csrf });
    }

    let (iss, sub) = match opt_auth.as_ref().map(|e| &e.0) {
        Some(AuthContext::User { iss, sub, .. }) => (iss.as_str(), sub.as_str()),
        _ => return StatusCode::FORBIDDEN.into_response(),
    };
    let user = match users::find_by_iss_sub(&state.pool, iss, sub).await {
        Ok(Some(u)) => u,
        Ok(None) => return StatusCode::FORBIDDEN.into_response(),
        Err(e) => {
            tracing::error!("find_by_iss_sub failed in orgs_index: {e:#}");
            return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load orgs.");
        }
    };
    let memberships = match list_memberships(&state.pool, user.user_id).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("list_memberships failed in orgs_index: {e:#}");
            return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load orgs.");
        }
    };
    let rows = memberships
        .into_iter()
        .map(|m| {
            let kind = OrgKind::classify(m.org_id, m.is_personal, m.ext_iss.is_some());
            OrgRow {
                org_id: m.org_id,
                name: m.name.unwrap_or_else(|| m.slug.clone()),
                slug: m.slug,
                kind: kind.label().to_owned(),
                role: Some(m.role),
                active: m.org_id == active_org.org_id,
            }
        })
        .collect();
    render_template(&OrgsTemplate { rows, show_create: true, csrf_token: csrf })
}

#[derive(Deserialize)]
pub struct CreateOrgForm {
    name: String,
    #[serde(default)]
    slug: Option<String>,
}

/// `POST /web/organizations`: create a native org owned by the caller. Regular users only.
pub async fn create_org(
    State(state): State<AppState>,
    _active_org: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Form(form): Form<CreateOrgForm>,
) -> axum::response::Response {
    // Only a real user can own an org; the admin token and every other context get 403.
    let (iss, sub) = match opt_auth.as_ref().map(|e| &e.0) {
        Some(AuthContext::User { iss, sub, .. }) => (iss.as_str(), sub.as_str()),
        _ => return StatusCode::FORBIDDEN.into_response(),
    };

    let name = form.name.trim();
    if name.is_empty() {
        return html_error(StatusCode::BAD_REQUEST, "Organization name is required.");
    }

    let user = match users::find_by_iss_sub(&state.pool, iss, sub).await {
        Ok(Some(u)) => u,
        Ok(None) => return StatusCode::FORBIDDEN.into_response(),
        Err(e) => {
            tracing::error!("find_by_iss_sub failed in create_org: {e:#}");
            return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create organization.");
        }
    };

    let cap = state.config.filter.max_native_orgs_per_user;
    match count_user_native_orgs(&state.pool, user.user_id).await {
        Ok(n) if n >= i64::from(cap) => {
            return html_error(
                StatusCode::FORBIDDEN,
                &format!("You have reached the limit of {cap} organizations."),
            );
        }
        Ok(_) => {}
        Err(e) => {
            tracing::error!("count_user_native_orgs failed: {e:#}");
            return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create organization.");
        }
    }

    let slug_src = form
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(name);
    let slug = slugify(slug_src);

    match create_native_org(&state.pool, user.user_id, &slug, name).await {
        Ok(_) => Redirect::to("/web/organizations").into_response(),
        Err(e) => {
            tracing::error!("create_native_org failed: {e:#}");
            html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create organization.")
        }
    }
}

#[derive(Deserialize)]
pub struct RoleForm {
    role: String,
}

/// Auth-before-kind preamble: Ok only if caller owns the native org at path_org_id, else Err(response); order hides org kind from non-owners.
async fn check_native_org_owner(
    pool: &DbPool,
    active_org: &ActiveOrg,
    opt_auth: &Option<Extension<AuthContext>>,
    path_org_id: i64,
) -> Result<(), axum::response::Response> {
    let is_superuser = active_org.role.is_none();
    if !is_superuser {
        let (iss, sub) = match opt_auth.as_ref().map(|e| &e.0) {
            Some(AuthContext::User { iss, sub, .. }) => (iss.as_str(), sub.as_str()),
            _ => return Err(StatusCode::FORBIDDEN.into_response()),
        };
        let user = match users::find_by_iss_sub(pool, iss, sub).await {
            Ok(Some(u)) => u,
            Ok(None) => return Err(StatusCode::FORBIDDEN.into_response()),
            Err(e) => {
                tracing::error!("find_by_iss_sub failed in org member mutation: {e:#}");
                return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
        };
        // 404 for non-member hides org existence; 403 only for confirmed members.
        match member_role(pool, user.user_id, path_org_id).await {
            Ok(Some(Role::Owner)) => {}
            Ok(Some(Role::Member)) => return Err(StatusCode::FORBIDDEN.into_response()),
            Ok(None) => return Err(StatusCode::NOT_FOUND.into_response()),
            Err(e) => {
                tracing::error!("member_role check failed: {e:#}");
                return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
        }
    }
    // Superuser or confirmed owner: verify org exists and is native.
    let org = match get_org(pool, path_org_id).await {
        Ok(Some(o)) => o,
        Ok(None) => return Err(StatusCode::NOT_FOUND.into_response()),
        Err(e) => {
            tracing::error!("get_org failed in org member mutation: {e:#}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };
    if OrgKind::classify(org.org_id, org.is_personal, org.ext_iss.is_some()) != OrgKind::Native {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    Ok(())
}

/// `POST /web/organizations/{org_id}/members/{user_id}/remove`: remove a member from the org.
pub async fn remove_org_member(
    State(state): State<AppState>,
    active_org: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Path((org_id, target_user_id)): Path<(i64, i64)>,
) -> axum::response::Response {
    if let Err(resp) = check_native_org_owner(&state.pool, &active_org, &opt_auth, org_id).await {
        return resp;
    }
    match member_role(&state.pool, target_user_id, org_id).await {
        Ok(None) => return Redirect::to(&format!("/web/organizations/{org_id}/members")).into_response(),
        Ok(Some(_)) => {}
        Err(e) => {
            tracing::error!("member_role check failed in remove_org_member: {e:#}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    match remove_member_guarded(&state.pool, target_user_id, org_id).await {
        Ok(0) => html_error(StatusCode::FORBIDDEN, "cannot remove the last owner"),
        Ok(_) => Redirect::to(&format!("/web/organizations/{org_id}/members")).into_response(),
        Err(e) => {
            tracing::error!("remove_member_guarded failed: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `POST /web/organizations/{org_id}/members/{user_id}/role`: change a member's role.
pub async fn set_org_member_role(
    State(state): State<AppState>,
    active_org: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Path((org_id, target_user_id)): Path<(i64, i64)>,
    Form(form): Form<RoleForm>,
) -> axum::response::Response {
    if let Err(resp) = check_native_org_owner(&state.pool, &active_org, &opt_auth, org_id).await {
        return resp;
    }
    let new_role = Role::parse(&form.role);
    match member_role(&state.pool, target_user_id, org_id).await {
        Ok(None) => return Redirect::to(&format!("/web/organizations/{org_id}/members")).into_response(),
        Ok(Some(_)) => {}
        Err(e) => {
            tracing::error!("member_role check failed in set_org_member_role: {e:#}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    match set_member_role_guarded(&state.pool, target_user_id, org_id, new_role).await {
        Ok(0) if new_role == Role::Member => {
            html_error(StatusCode::FORBIDDEN, "cannot demote the last owner")
        }
        Ok(_) => Redirect::to(&format!("/web/organizations/{org_id}/members")).into_response(),
        Err(e) => {
            tracing::error!("set_member_role_guarded failed: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `POST /web/organizations/{org_id}/invites/{invite_id}/revoke`: revoke a pending invite.
pub async fn revoke_org_invite(
    State(state): State<AppState>,
    active_org: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Path((org_id, invite_id)): Path<(i64, i64)>,
) -> axum::response::Response {
    if let Err(resp) = check_native_org_owner(&state.pool, &active_org, &opt_auth, org_id).await {
        return resp;
    }
    if let Err(e) = revoke_invite(&state.pool, invite_id, org_id).await {
        tracing::error!("revoke_invite failed: {e:#}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Redirect::to(&format!("/web/organizations/{org_id}/members")).into_response()
}

/// `GET /web/organizations/{org_id}/members`: list members and pending invites for an org.
pub async fn org_members(
    State(state): State<AppState>,
    active_org: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Path(org_id): Path<i64>,
    Csrf(csrf): Csrf,
) -> impl IntoResponse {
    let is_superuser = active_org.role.is_none();

    let caller_id: Option<i64> = if is_superuser {
        None
    } else {
        let (iss, sub) = match opt_auth.as_ref().map(|e| &e.0) {
            Some(AuthContext::User { iss, sub, .. }) => (iss.as_str(), sub.as_str()),
            _ => return StatusCode::FORBIDDEN.into_response(),
        };
        let user = match users::find_by_iss_sub(&state.pool, iss, sub).await {
            Ok(Some(u)) => u,
            Ok(None) => return StatusCode::FORBIDDEN.into_response(),
            Err(e) => {
                tracing::error!("find_by_iss_sub failed in org_members: {e:#}");
                return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load members.");
            }
        };
        Some(user.user_id)
    };

    // View gate: non-members get 404 to avoid leaking org existence.
    if !is_superuser {
        match member_role(&state.pool, caller_id.unwrap(), org_id).await {
            Ok(None) => return html_error(StatusCode::NOT_FOUND, "Organization not found."),
            Ok(Some(_)) => {}
            Err(e) => {
                tracing::error!("member_role check failed in org_members: {e:#}");
                return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load members.");
            }
        }
    }

    let org = match get_org(&state.pool, org_id).await {
        Ok(Some(o)) => o,
        Ok(None) => return html_error(StatusCode::NOT_FOUND, "Organization not found."),
        Err(e) => {
            tracing::error!("get_org failed in org_members: {e:#}");
            return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load org.");
        }
    };

    let kind = OrgKind::classify(org.org_id, org.is_personal, org.ext_iss.is_some());
    let is_native = kind == OrgKind::Native;

    let is_org_owner = if is_superuser {
        true
    } else {
        match is_current_owner(&state.pool, caller_id.unwrap(), org_id).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("is_current_owner check failed in org_members: {e:#}");
                return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load members.");
            }
        }
    };
    let can_manage = is_org_owner && is_native;
    // Owners may rename native and personal org slugs; system/Forseti slugs are managed elsewhere.
    let can_rename = is_org_owner && matches!(kind, OrgKind::Native | OrgKind::Personal);
    let current_slug = org.slug.clone();

    let raw_members = match list_org_members(&state.pool, org_id).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("list_org_members failed in org_members: {e:#}");
            return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load members.");
        }
    };

    let owner_count = if can_manage {
        match count_owners(&state.pool, org_id).await {
            Ok(n) => n,
            Err(e) => {
                tracing::error!("count_owners failed in org_members: {e:#}");
                return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load members.");
            }
        }
    } else {
        0
    };

    let members: Vec<MemberView> = raw_members
        .into_iter()
        .map(|m| {
            let name = m
                .name
                .as_deref()
                .filter(|n| !n.is_empty())
                .map(str::to_owned)
                .or_else(|| m.email.clone().filter(|e| !e.is_empty()))
                .unwrap_or_else(|| format!("user #{}", m.user_id));
            let is_owner = m.role == "owner";
            let can_remove = can_manage && !strands_org(is_owner, owner_count);
            let can_demote = can_manage && !strands_org(is_owner, owner_count);
            MemberView {
                user_id: m.user_id,
                name,
                email: m.email,
                role: m.role,
                joined_at: m.joined_at,
                can_remove,
                can_demote,
            }
        })
        .collect();

    let invites = if can_manage {
        let now = chrono::Utc::now().timestamp();
        match list_org_invites(&state.pool, org_id).await {
            Ok(rows) => rows
                .into_iter()
                .map(|r| {
                    let status = if r.accepted_at.is_some() {
                        "accepted".to_owned()
                    } else if now > r.expires_at {
                        "expired".to_owned()
                    } else {
                        "pending".to_owned()
                    };
                    InviteView {
                        invite_id: r.invite_id,
                        role: r.role,
                        email: r.email,
                        created_at: r.created_at,
                        expires_at: r.expires_at,
                        status,
                    }
                })
                .collect(),
            Err(e) => {
                tracing::error!("list_org_invites failed in org_members: {e:#}");
                return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load invites.");
            }
        }
    } else {
        vec![]
    };

    let can_delete = is_org_owner && matches!(kind, OrgKind::Native | OrgKind::Forseti);
    let member_count = members.len() as i64;
    let project_count = if can_delete {
        match count_projects_in_org(&state.pool, org_id).await {
            Ok(n) => n,
            Err(e) => {
                tracing::error!("count_projects_in_org failed in org_members: {e:#}");
                return html_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load members.");
            }
        }
    } else {
        0
    };
    let slug = current_slug.clone();

    let org_name = org.name.unwrap_or_else(|| org.slug.clone());
    render_template(&OrgMembersTemplate {
        org_id,
        org_name,
        kind: kind.label().to_owned(),
        is_native,
        can_manage,
        can_rename,
        current_slug,
        members,
        invites,
        csrf_token: csrf,
        can_delete,
        project_count,
        member_count,
        slug,
    })
}

#[derive(Deserialize)]
pub struct DeleteOrgForm {
    confirm_slug: String,
}

/// `POST /web/organizations/{org_id}/delete`: hard-delete a native or Forseti org.
pub async fn delete_org(
    State(state): State<AppState>,
    active_org: ActiveOrg,
    opt_auth: Option<Extension<AuthContext>>,
    Path(org_id): Path<i64>,
    Form(form): Form<DeleteOrgForm>,
) -> axum::response::Response {
    let is_superuser = active_org.role.is_none();

    let org = match get_org(&state.pool, org_id).await {
        Ok(Some(o)) => o,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("get_org failed in delete_org: {e:#}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if !is_superuser {
        let (iss, sub) = match opt_auth.as_ref().map(|e| &e.0) {
            Some(AuthContext::User { iss, sub, .. }) => (iss.as_str(), sub.as_str()),
            _ => return StatusCode::FORBIDDEN.into_response(),
        };
        let user = match users::find_by_iss_sub(&state.pool, iss, sub).await {
            Ok(Some(u)) => u,
            Ok(None) => return StatusCode::FORBIDDEN.into_response(),
            Err(e) => {
                tracing::error!("find_by_iss_sub failed in delete_org: {e:#}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
        match is_current_owner(&state.pool, user.user_id, org_id).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::NOT_FOUND.into_response(),
            Err(e) => {
                tracing::error!("is_current_owner failed in delete_org: {e:#}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    if form.confirm_slug != org.slug {
        return html_error(StatusCode::BAD_REQUEST, "Type the organization slug to confirm deletion.");
    }

    let kind = OrgKind::classify(org.org_id, org.is_personal, org.ext_iss.is_some());
    match delete_org_guarded(&state.writer_pool, org_id).await {
        Ok(DeleteOrgOutcome::NotDeletable) => {
            html_error(StatusCode::BAD_REQUEST, "This organization cannot be deleted.")
        }
        Ok(DeleteOrgOutcome::Deleted(counts)) => {
            let actor = opt_auth
                .as_ref()
                .map(|e| e.0.source())
                .map(|s| format!("{s:?}"))
                .unwrap_or_else(|| "unknown".to_owned());
            tracing::warn!(
                target: "stackpit::audit",
                actor = %actor,
                org_id,
                org_kind = ?kind,
                projects = counts.projects,
                members = counts.members,
                invites = counts.invites,
                integrations = counts.integrations,
                alert_rules = counts.alert_rules,
                digest_schedules = counts.digest_schedules,
                "organization deleted"
            );

            let mut resp = Redirect::to("/web/organizations").into_response();
            if active_org.org_id == org_id {
                let secure = state.config.server.cookies_should_be_secure();
                let secure_flag = if secure { "; Secure" } else { "" };
                let clear = format!(
                    "{ACTIVE_ORG_COOKIE}=; Path=/; SameSite=Strict; HttpOnly; Max-Age=0{secure_flag}"
                );
                if let Ok(val) = clear.parse() {
                    resp.headers_mut().append("set-cookie", val);
                }
            }
            resp
        }
        Err(e) => {
            tracing::error!("delete_org_guarded failed: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_member_org_is_rejected() {
        assert!(!can_switch_to(42, &[10, 11], false));
    }

    #[test]
    fn member_org_is_allowed() {
        assert!(can_switch_to(11, &[10, 11], false));
    }

    #[test]
    fn system_org_rejected_for_normal_user() {
        // Even if somehow a user is listed as a member of org 1, they can't switch to it.
        assert!(!can_switch_to(SYSTEM_ORG_ID, &[SYSTEM_ORG_ID, 10], false));
    }

    #[test]
    fn superuser_may_switch_to_any_org_including_system() {
        assert!(can_switch_to(SYSTEM_ORG_ID, &[], true));
        assert!(can_switch_to(42, &[], true));
        assert!(can_switch_to(99, &[], true));
    }

    #[test]
    fn strands_org_sole_owner_strands() {
        assert!(strands_org(true, 1));
    }

    #[test]
    fn strands_org_owner_with_second_owner_does_not_strand() {
        assert!(!strands_org(true, 2));
    }

    #[test]
    fn strands_org_non_owner_never_strands() {
        assert!(!strands_org(false, 1));
    }

    // Browsers submit a blank optional expiry as `ttl_secs=`; that must default,
    // not 400 (regression: F4).
    #[test]
    fn invite_form_blank_ttl_deserializes_to_none() {
        let form: CreateInviteForm =
            serde_urlencoded::from_str("role=member&ttl_secs=").expect("blank ttl must parse");
        assert_eq!(form.role, "member");
        assert_eq!(form.ttl_secs, None);
    }

    #[test]
    fn invite_form_numeric_ttl_parses() {
        let form: CreateInviteForm =
            serde_urlencoded::from_str("role=member&ttl_secs=3600").expect("numeric ttl must parse");
        assert_eq!(form.ttl_secs, Some(3600));
    }

    #[test]
    fn invite_form_invalid_ttl_still_errors() {
        let res: Result<CreateInviteForm, _> = serde_urlencoded::from_str("role=member&ttl_secs=abc");
        assert!(res.is_err(), "non-numeric ttl must not be silently swallowed");
    }
}
