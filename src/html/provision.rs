use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use stackpit_auth::read_cookie;
use stackpit_auth::AuthContext;

use crate::html::chrome::Localized;
use crate::html::utils::Chrome;
use crate::locale::LanguageIdentifier;
use crate::oidc::client::OrgClaim;
use crate::queries::{orgs as orgs_queries, users};
use crate::server::AppState;
use crate::util::crypto::SecretEncryptor;

pub const PROVISION_COOKIE: &str = "sp_provision";
const AAD: &[u8] = b"stackpit:provision:v1";
const PROVISION_TTL_SECS: i64 = 900;

#[derive(Serialize, Deserialize)]
pub struct ProvisionState {
    pub orgs: Vec<OrgClaim>,
    pub iss: String,
    pub expires_at: i64,
    /// Owner of the cookie, bound when the interstitial is first rendered. A
    /// POST from any other session (shared browser) is refused, as is an
    /// unbound cookie.
    #[serde(default)]
    pub user_id: Option<i64>,
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

/// Build a fresh ProvisionState cookie blob for the given orgs + issuer, bound
/// to the user it was minted for.
pub fn new_state(orgs: Vec<OrgClaim>, iss: String, user_id: i64) -> ProvisionState {
    ProvisionState {
        orgs,
        iss,
        expires_at: chrono::Utc::now().timestamp() + PROVISION_TTL_SECS,
        user_id: Some(user_id),
    }
}

/// Returns only ids present in BOTH the signed set and the submitted set.
pub fn intersect_provisionable(signed: &[String], submitted: &[String]) -> Vec<String> {
    submitted
        .iter()
        .filter(|id| signed.contains(id))
        .cloned()
        .collect()
}

/// `SameSite=Lax`, matching the grant cookie: this is set on the IdP callback
/// response and the very next hop is the redirect to `/web/provision`, which is
/// the tail of a cross-site redirect chain. `Strict` withholds it there and the
/// interstitial can never render.
pub fn build_provision_cookie(blob: &str, secure: bool) -> HeaderValue {
    let secure_flag = if secure { "; Secure" } else { "" };
    let v = format!(
        "{PROVISION_COOKIE}={blob}; Path=/web/provision; SameSite=Lax; HttpOnly; \
         Max-Age={PROVISION_TTL_SECS}{secure_flag}"
    );
    HeaderValue::from_str(&v).expect("provision cookie is valid ASCII")
}

fn clear_provision_cookie(secure: bool) -> HeaderValue {
    let secure_flag = if secure { "; Secure" } else { "" };
    let v = format!(
        "{PROVISION_COOKIE}=; Path=/web/provision; SameSite=Lax; HttpOnly; Max-Age=0{secure_flag}"
    );
    HeaderValue::from_str(&v).expect("clear provision cookie is valid ASCII")
}

#[derive(askama::Template)]
#[template(path = "provision.html")]
struct ProvisionTemplate {
    orgs: Vec<OrgClaim>,
    csrf_token: String,
    /// Standalone page (no PageChrome), so it carries its own locale and looks
    /// strings up directly.
    locale: LanguageIdentifier,
}

impl Localized for ProvisionTemplate {
    fn locale(&self) -> &LanguageIdentifier {
        &self.locale
    }
}

/// `GET /web/provision` -- render the provisioning interstitial from the signed
/// cookie, binding the cookie to the viewing user on the way out.
pub async fn provision_form(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    opt_auth: Option<Extension<AuthContext>>,
    headers: HeaderMap,
) -> Response {
    let Some(enc) = state.encryptor.as_deref() else {
        return Redirect::to("/web/").into_response();
    };
    let Some(blob) = read_cookie(&headers, PROVISION_COOKIE) else {
        return Redirect::to("/web/").into_response();
    };
    let Some(mut ps) = unpack(enc, blob) else {
        return Redirect::to("/web/").into_response();
    };
    if chrono::Utc::now().timestamp() > ps.expires_at {
        return Redirect::to("/web/").into_response();
    }

    let viewer = match opt_auth.as_ref().map(|e| &e.0) {
        Some(AuthContext::User { iss, sub, .. }) => {
            match users::find_by_iss_sub(&state.pool, iss, sub).await {
                Ok(Some(u)) => Some(u.user_id),
                Ok(None) => None,
                Err(e) => {
                    tracing::error!("find_by_iss_sub failed in provision_form: {e:#}");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
        }
        _ => None,
    };

    // A cookie already bound to someone else must not be shown, let alone rebound.
    let foreign = matches!((ps.user_id, viewer), (Some(owner), Some(v)) if owner != v);
    if foreign {
        let mut resp = Redirect::to("/web/").into_response();
        resp.headers_mut().append(
            SET_COOKIE,
            clear_provision_cookie(state.config.server.cookies_should_be_secure()),
        );
        return resp;
    }

    let rebound_cookie = if ps.user_id.is_none() && viewer.is_some() {
        ps.user_id = viewer;
        let secure = state.config.server.cookies_should_be_secure();
        pack(enc, &ps).map(|blob| build_provision_cookie(&blob, secure))
    } else {
        None
    };

    let mut resp = crate::html::render_template(&ProvisionTemplate {
        orgs: ps.orgs,
        csrf_token: chrome.csrf_token,
        locale: chrome.locale,
    });
    if let Some(cookie) = rebound_cookie {
        resp.headers_mut().append(SET_COOKIE, cookie);
    }
    resp
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
        resp.headers_mut()
            .append(SET_COOKIE, clear_provision_cookie(secure));
        return resp;
    };
    if chrono::Utc::now().timestamp() > ps.expires_at {
        let mut resp = Redirect::to("/web/").into_response();
        resp.headers_mut()
            .append(SET_COOKIE, clear_provision_cookie(secure));
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

    // A cookie left behind by an earlier session must not provision for whoever
    // holds the browser now; unbound cookies are refused for the same reason.
    if ps.user_id != Some(user.user_id) {
        tracing::warn!(
            target: "stackpit::audit",
            user_id = user.user_id,
            "refused provision: sp_provision cookie is not bound to this session"
        );
        let mut resp = StatusCode::FORBIDDEN.into_response();
        resp.headers_mut()
            .append(SET_COOKIE, clear_provision_cookie(secure));
        return resp;
    }

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
            &state.writer_pool,
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
    resp.headers_mut()
        .append(SET_COOKIE, clear_provision_cookie(secure));
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;
    use unic_langid::langid;

    #[test]
    fn provision_renders_without_missing_keys() {
        for locale in [langid!("en"), langid!("de")] {
            let tmpl = ProvisionTemplate {
                orgs: Vec::new(),
                csrf_token: "tok-123".into(),
                locale: locale.clone(),
            };
            let html = tmpl.render().expect("provision renders");
            assert!(
                !html.contains(crate::i18n::MISSING_PREFIX),
                "provision ({locale}) leaked a missing localization key: {html}"
            );
            // Without this field the POST is rejected by csrf_middleware.
            assert!(
                html.contains(r#"name="csrf_token" value="tok-123""#),
                "provision ({locale}) form must carry the csrf token: {html}"
            );
        }
    }

    #[test]
    fn provision_cookie_survives_the_idp_redirect_like_the_grant_cookie() {
        // Both are set on the /web/auth/callback response, and the next hop is a
        // cross-site redirect. Anything stricter than the grant cookie is dropped
        // there, leaving the interstitial permanently unreachable.
        let grant = crate::oidc::cookies::build_grant_cookie("deadbeef", false);
        assert!(grant.to_str().unwrap().contains("SameSite=Lax"));

        for cookie in [
            build_provision_cookie("blob", false),
            clear_provision_cookie(false),
        ] {
            let v = cookie.to_str().unwrap();
            assert!(
                v.contains("SameSite=Lax"),
                "provision cookie must match the grant cookie's SameSite: {v}"
            );
            assert!(
                v.contains("HttpOnly"),
                "provision cookie must stay HttpOnly: {v}"
            );
        }
    }

    #[tokio::test]
    async fn provision_post_is_rejected_without_csrf_token() {
        use axum::body::Body;
        use axum::http::Request;
        use axum::routing::post;
        use axum::Router;
        use tower::ServiceExt;

        async fn inject(
            mut req: Request<Body>,
            next: axum::middleware::Next,
        ) -> axum::response::Response {
            req.extensions_mut()
                .insert(crate::middleware::CsrfToken("tok-123".to_owned()));
            next.run(req).await
        }

        let app = Router::new()
            .route("/web/provision", post(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn_with_state(
                crate::middleware::CsrfConfig {
                    max_body_size: 64 * 1024,
                },
                crate::middleware::csrf_middleware,
            ))
            .layer(axum::middleware::from_fn(inject));

        let submit = |body: &'static str| {
            app.clone().oneshot(
                Request::post("/web/provision")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(body))
                    .unwrap(),
            )
        };

        let missing = submit("org_ids=acme").await.unwrap();
        assert_eq!(
            missing.status(),
            StatusCode::FORBIDDEN,
            "/web/provision is in CSRF scope: a token-less POST must 403"
        );

        let present = submit("org_ids=acme&csrf_token=tok-123").await.unwrap();
        assert_ne!(
            present.status(),
            StatusCode::FORBIDDEN,
            "a valid csrf_token must reach the handler"
        );
    }

    // Cookies minted before the binding landed (and any hand-rolled one) must
    // decode as unbound, which provision_submit refuses.
    #[test]
    fn legacy_cookie_state_decodes_as_unbound() {
        let legacy =
            r#"{"orgs":[],"iss":"https://idp","expires_at":9999999999,"nonce":"deadbeef"}"#;
        let ps: ProvisionState = serde_json::from_str(legacy).expect("legacy state decodes");
        assert_eq!(ps.user_id, None);
    }

    #[test]
    fn new_state_is_bound_to_the_minting_user() {
        let ps = new_state(Vec::new(), "https://idp".into(), 42);
        assert_eq!(ps.user_id, Some(42));
        assert!(ps.expires_at > chrono::Utc::now().timestamp());
    }

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
