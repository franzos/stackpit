use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use stackpit_auth::AuthContext;
use stackpit_auth::read_cookie;

use crate::oidc::client::OrgClaim;
use crate::queries::{orgs as orgs_queries, users};
use crate::server::AppState;
use crate::util::crypto::{random_hex, SecretEncryptor};

pub const PROVISION_COOKIE: &str = "sp_provision";
const AAD: &[u8] = b"stackpit:provision:v1";
const PROVISION_TTL_SECS: i64 = 900;

#[derive(Serialize, Deserialize)]
pub struct ProvisionState {
    pub orgs: Vec<OrgClaim>,
    pub iss: String,
    pub expires_at: i64,
    pub nonce: String,
}

pub fn pack(enc: &SecretEncryptor, s: &ProvisionState) -> Option<String> {
    let json = serde_json::to_vec(s).ok()?;
    let ct = enc.encrypt_bytes_with_aad(&json, AAD)?;
    Some(URL_SAFE_NO_PAD.encode(ct))
}

pub fn unpack(enc: &SecretEncryptor, blob_b64: &str) -> Option<ProvisionState> {
    let ct = URL_SAFE_NO_PAD.decode(blob_b64.trim()).ok()?;
    let pt = enc.decrypt_bytes_with_aad(&ct, AAD)?;
    serde_json::from_slice(&pt).ok()
}

/// Build a fresh ProvisionState cookie blob for the given orgs + issuer.
pub fn new_state(orgs: Vec<OrgClaim>, iss: String) -> ProvisionState {
    ProvisionState {
        orgs,
        iss,
        expires_at: chrono::Utc::now().timestamp() + PROVISION_TTL_SECS,
        nonce: random_hex::<16>(),
    }
}

/// Returns only ids present in BOTH the signed set and the submitted set.
pub fn intersect_provisionable(signed: &[String], submitted: &[String]) -> Vec<String> {
    submitted.iter().filter(|id| signed.contains(id)).cloned().collect()
}

pub fn build_provision_cookie(blob: &str, secure: bool) -> HeaderValue {
    let secure_flag = if secure { "; Secure" } else { "" };
    let v = format!(
        "{PROVISION_COOKIE}={blob}; Path=/web/provision; SameSite=Strict; HttpOnly; \
         Max-Age={PROVISION_TTL_SECS}{secure_flag}"
    );
    HeaderValue::from_str(&v).expect("provision cookie is valid ASCII")
}

fn clear_provision_cookie(secure: bool) -> HeaderValue {
    let secure_flag = if secure { "; Secure" } else { "" };
    let v = format!(
        "{PROVISION_COOKIE}=; Path=/web/provision; SameSite=Strict; HttpOnly; Max-Age=0{secure_flag}"
    );
    HeaderValue::from_str(&v).expect("clear provision cookie is valid ASCII")
}

#[derive(askama::Template)]
#[template(path = "provision.html")]
struct ProvisionTemplate {
    orgs: Vec<OrgClaim>,
}

/// `GET /web/provision` -- render the provisioning interstitial from the signed cookie.
pub async fn provision_form(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(enc) = state.encryptor.as_deref() else {
        return Redirect::to("/web/").into_response();
    };
    let Some(blob) = read_cookie(&headers, PROVISION_COOKIE) else {
        return Redirect::to("/web/").into_response();
    };
    let Some(ps) = unpack(enc, blob) else {
        return Redirect::to("/web/").into_response();
    };
    if chrono::Utc::now().timestamp() > ps.expires_at {
        return Redirect::to("/web/").into_response();
    }
    crate::html::render_template(&ProvisionTemplate { orgs: ps.orgs })
}

/// `POST /web/provision` -- validate cookie, intersect submitted ids with signed set, provision.
pub async fn provision_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    opt_auth: Option<Extension<AuthContext>>,
    body: Bytes,
) -> Response {
    let secure = state.config.server.cookies_should_be_secure();

    let Some(enc) = state.encryptor.as_deref() else {
        return Redirect::to("/web/").into_response();
    };
    let Some(blob) = read_cookie(&headers, PROVISION_COOKIE) else {
        return Redirect::to("/web/").into_response();
    };
    let Some(ps) = unpack(enc, blob) else {
        let mut resp = Redirect::to("/web/").into_response();
        resp.headers_mut().append(SET_COOKIE, clear_provision_cookie(secure));
        return resp;
    };
    if chrono::Utc::now().timestamp() > ps.expires_at {
        let mut resp = Redirect::to("/web/").into_response();
        resp.headers_mut().append(SET_COOKIE, clear_provision_cookie(secure));
        return resp;
    }

    let (auth_iss, auth_sub) = match opt_auth.as_ref().map(|e| &e.0) {
        Some(AuthContext::User { iss, sub, .. }) => (iss.as_str(), sub.as_str()),
        _ => return StatusCode::FORBIDDEN.into_response(),
    };

    let user = match users::find_by_iss_sub(&state.pool, auth_iss, auth_sub).await {
        Ok(Some(u)) => u,
        Ok(None) => return StatusCode::FORBIDDEN.into_response(),
        Err(e) => {
            tracing::error!("find_by_iss_sub failed during provision: {e:#}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Collect submitted org_ids from the raw form body.
    let submitted: Vec<String> = form_urlencoded::parse(&body)
        .filter(|(k, _)| k == "org_ids")
        .map(|(_, v)| v.into_owned())
        .collect();

    let signed_ids: Vec<String> = ps.orgs.iter().map(|o| o.id.clone()).collect();
    let allowed_ids = intersect_provisionable(&signed_ids, &submitted);

    for id in &allowed_ids {
        let Some(claim) = ps.orgs.iter().find(|o| &o.id == id) else {
            continue;
        };
        let name = claim.name.as_deref().unwrap_or(claim.slug.as_str());
        // iss from the signed cookie, never from the form or AuthContext
        if let Err(e) = orgs_queries::provision_forseti_org(
            &state.pool,
            user.user_id,
            &ps.iss,
            &claim.id,
            &claim.slug,
            name,
        )
        .await
        {
            tracing::error!("provision_forseti_org failed for org {}: {e:#}", claim.id);
        }
    }

    // Single-use: clear the cookie on every POST (success, partial, or skip).
    let mut resp = Redirect::to("/web/").into_response();
    resp.headers_mut().append(SET_COOKIE, clear_provision_cookie(secure));
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provision_selection_is_intersected_with_signed_set() {
        let signed = vec!["acme".to_string(), "widgets".to_string()];
        let submitted = vec!["acme".to_string(), "evilcorp".to_string()];
        let allowed = intersect_provisionable(&signed, &submitted);
        assert_eq!(allowed, vec!["acme".to_string()]); // evilcorp dropped: not in signed set
    }

    #[test]
    fn empty_submission_yields_empty() {
        let signed = vec!["acme".to_string()];
        let allowed = intersect_provisionable(&signed, &[]);
        assert!(allowed.is_empty());
    }

    #[test]
    fn empty_signed_set_yields_empty() {
        let submitted = vec!["acme".to_string()];
        let allowed = intersect_provisionable(&[], &submitted);
        assert!(allowed.is_empty());
    }
}
