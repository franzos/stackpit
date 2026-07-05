//! /web/admin/license — view licensee identity + status, activate a new blob,
//! deactivate. Superuser-only. Lives in commercial/ for the license boundary.

use askama::Template;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;

use crate::commercial::license::{classify, LicenseStatus};
use crate::commercial::{store, verify};
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::Chrome;
use crate::orgs::extractor::{require_superuser, ActiveOrg};
use crate::server::AppState;

/// Keeps the license URL table inside `src/commercial/` (the LICENSE-COMMERCIAL
/// boundary) rather than inlined among the MIT routes in `html/mod.rs`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/web/admin/license", get(view))
        .route("/web/admin/license/activate", post(activate))
        .route("/web/admin/license/deactivate", post(deactivate))
}

#[derive(Template)]
#[template(path = "admin_license.html")]
struct LicenseTemplate {
    chrome: PageChrome,
    state_label: &'static str,
    has_license: bool,
    customer: String,
    email: String,
    issued_at: String,
    expires_at: String,
    is_lifetime: bool,
    max_orgs_label: String,
    banner_kind: &'static str,
    banner_message: String,
}

async fn view(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
) -> Response {
    if let Err(r) = require_superuser(&active) {
        return r;
    }
    let status = state.license.status();
    let tpl = build_template(chrome, &status, state.license.grace_days());
    render_template(&tpl).into_response()
}

#[derive(Deserialize)]
struct ActivateForm {
    blob: String,
    #[allow(dead_code)] // consumed by csrf_middleware ahead of this handler
    csrf_token: String,
}

async fn activate(
    State(state): State<AppState>,
    active: ActiveOrg,
    axum::Form(form): axum::Form<ActivateForm>,
) -> Response {
    if let Err(r) = require_superuser(&active) {
        return r;
    }
    match verify::decode_and_verify(&form.blob) {
        Ok(license) => {
            let next = classify(
                license.clone(),
                state.license.grace_days(),
                chrono::Utc::now(),
            );
            match store::save(&state.writer_pool, &form.blob, &license).await {
                Ok(()) => {
                    state.license.swap(next);
                    tracing::info!(customer = %license.customer, email = %license.email, "license: activated");
                }
                Err(e) => tracing::error!(error = ?e, "license: persist failed"),
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "license: activation rejected: {}", verify::user_message(&e))
        }
    }
    Redirect::to("/web/admin/license").into_response()
}

#[derive(Deserialize)]
struct DeactivateForm {
    #[allow(dead_code)] // consumed by csrf_middleware ahead of this handler
    csrf_token: String,
}

async fn deactivate(
    State(state): State<AppState>,
    active: ActiveOrg,
    axum::Form(form): axum::Form<DeactivateForm>,
) -> Response {
    if let Err(r) = require_superuser(&active) {
        return r;
    }
    let _ = &form.csrf_token;
    match store::clear(&state.writer_pool).await {
        Ok(()) => state.license.swap(LicenseStatus::Unlicensed),
        Err(e) => tracing::error!(error = ?e, "license: deactivate failed"),
    }
    Redirect::to("/web/admin/license").into_response()
}

fn build_template(chrome: PageChrome, status: &LicenseStatus, grace_days: i64) -> LicenseTemplate {
    let (state_label, banner_kind, banner_message): (&'static str, &'static str, String) =
        match status {
            LicenseStatus::Unlicensed => ("Unlicensed", "", String::new()),
            LicenseStatus::Active(_) => ("Active", "", String::new()),
            LicenseStatus::Grace(l) => {
                let now = chrono::Utc::now();
                let days_past = l.expires_at.map(|e| (now - e).num_days()).unwrap_or(0);
                let remaining = (grace_days - days_past).max(0);
                (
                    "Grace",
                    "warning",
                    format!("License expired, still accepted for {remaining} more day(s). Renew to keep premium features."),
                )
            }
            LicenseStatus::Expired(_) => (
                "Expired",
                "danger",
                "License expired and grace period ended.".into(),
            ),
        };
    match status.license() {
        Some(l) => LicenseTemplate {
            chrome,
            state_label,
            has_license: true,
            customer: l.customer.clone(),
            email: l.email.clone(),
            issued_at: l.issued_at.format("%Y-%m-%d").to_string(),
            expires_at: l
                .expires_at
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            is_lifetime: l.expires_at.is_none(),
            max_orgs_label: l
                .max_orgs
                .map_or_else(|| "Unlimited".into(), |n| n.to_string()),
            banner_kind,
            banner_message,
        },
        None => LicenseTemplate {
            chrome,
            state_label,
            has_license: false,
            customer: String::new(),
            email: String::new(),
            issued_at: String::new(),
            expires_at: String::new(),
            is_lifetime: false,
            max_orgs_label: String::new(),
            banner_kind,
            banner_message,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commercial::license::License;
    use chrono::Utc;
    use unic_langid::langid;

    fn active_status() -> LicenseStatus {
        LicenseStatus::Active(License {
            license_id: "abc".into(),
            customer: "Stackpit Test".into(),
            email: "test@stackpit.test".into(),
            issued_at: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            expires_at: chrono::DateTime::from_timestamp(1_900_000_000, 0),
            features: Vec::new(),
            max_orgs: Some(50),
        })
    }

    fn chrome(locale: unic_langid::LanguageIdentifier) -> PageChrome {
        PageChrome::new(
            "test-csrf-token".into(),
            locale,
            "/web/admin/license".into(),
        )
    }

    #[test]
    fn license_page_active_renders_stable() {
        let tpl = build_template(
            chrome(langid!("en")),
            &active_status(),
            crate::commercial::GRACE_DAYS,
        );
        insta::assert_snapshot!("license_active_en", tpl.render().unwrap());
    }

    #[test]
    fn license_page_unlicensed_renders_stable() {
        let tpl = build_template(
            chrome(langid!("en")),
            &LicenseStatus::Unlicensed,
            crate::commercial::GRACE_DAYS,
        );
        insta::assert_snapshot!("license_unlicensed_en", tpl.render().unwrap());
    }

    #[test]
    fn license_page_no_missing_keys() {
        let now = Utc::now();
        let grace = LicenseStatus::Grace(License {
            license_id: "g".into(),
            customer: "Grace Co".into(),
            email: "g@grace.test".into(),
            issued_at: now,
            expires_at: Some(now - chrono::Duration::days(5)),
            features: Vec::new(),
            max_orgs: None,
        });
        for locale in [langid!("en"), langid!("de")] {
            for status in [&active_status(), &LicenseStatus::Unlicensed, &grace] {
                let tpl = build_template(
                    chrome(locale.clone()),
                    status,
                    crate::commercial::GRACE_DAYS,
                );
                let html = tpl.render().expect("license page renders");
                assert!(
                    !html.contains(crate::i18n::MISSING_PREFIX),
                    "license page ({locale}) leaked a missing localization key: {html}"
                );
            }
        }
    }
}
