//! Licensed integration providers: Slack, generic webhooks, and the issue
//! trackers (GitHub / Forgejo / GitLab).
//!
//! These live under `src/commercial/` because they're gated by
//! [`Feature::Integrations`] and therefore governed by `LICENSE-COMMERCIAL`
//! rather than the MIT core. Email stays in `crate::providers` — it's the free
//! baseline channel, so an unlicensed install can still alert.

pub mod slack;
pub mod tracker;
pub mod webhook;

use anyhow::Result;

use super::license::{Feature, FeatureStatus};
use super::LicenseHandle;
use crate::domain::IntegrationKind;

/// Send a built request and bail if the response status isn't 2xx. `label`
/// names the provider in the error (e.g. "webhook", "slack webhook").
pub(crate) async fn send_and_check(req: reqwest::RequestBuilder, label: &str) -> Result<()> {
    let resp = req.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("{label} returned {}", resp.status());
    }
    Ok(())
}

/// Gate for one integration kind. Email is never gated, so it short-circuits to
/// [`FeatureStatus::Allowed`] before the license is consulted at all.
pub fn gate_kind(license: &LicenseHandle, kind: IntegrationKind) -> FeatureStatus {
    if !kind.requires_license() {
        return FeatureStatus::Allowed;
    }
    license.feature(Feature::Integrations)
}

/// True when a licensed kind may be *configured* — adding, editing, enabling,
/// or firing a one-off test. Grace is deliberately excluded: those are hard
/// POSTs, and the grace window only keeps existing delivery alive.
pub fn may_configure(license: &LicenseHandle, kind: IntegrationKind) -> bool {
    matches!(gate_kind(license, kind), FeatureStatus::Allowed)
}

/// True when an already-configured integration may still *deliver*. Grace
/// counts as allowed so a lapsed renewal doesn't silently stop production
/// alerting mid-window; only a fully expired (or absent) license stops it.
pub fn may_deliver(license: &LicenseHandle, kind: IntegrationKind) -> bool {
    matches!(
        gate_kind(license, kind),
        FeatureStatus::Allowed | FeatureStatus::GraceReadOnly
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commercial::license::{License, LicenseStatus};
    use crate::commercial::GRACE_DAYS;
    use chrono::{Duration, Utc};

    fn handle(status: LicenseStatus) -> LicenseHandle {
        LicenseHandle::new(status, GRACE_DAYS)
    }

    fn license_with(features: Vec<Feature>, expires_in_days: i64) -> License {
        License {
            license_id: "test".into(),
            customer: "Test Co".into(),
            email: "t@example.com".into(),
            issued_at: Utc::now(),
            expires_at: Some(Utc::now() + Duration::days(expires_in_days)),
            features,
            max_orgs: None,
        }
    }

    const LICENSED: [IntegrationKind; 5] = [
        IntegrationKind::Webhook,
        IntegrationKind::Slack,
        IntegrationKind::GitHub,
        IntegrationKind::Forgejo,
        IntegrationKind::GitLab,
    ];

    #[test]
    fn email_is_never_gated() {
        for status in [
            LicenseStatus::Unlicensed,
            LicenseStatus::Expired(license_with(vec![], -100)),
            LicenseStatus::Grace(license_with(vec![], -1)),
        ] {
            let h = handle(status);
            assert!(may_configure(&h, IntegrationKind::Email));
            assert!(may_deliver(&h, IntegrationKind::Email));
        }
    }

    #[test]
    fn unlicensed_blocks_every_licensed_kind() {
        let h = handle(LicenseStatus::Unlicensed);
        for kind in LICENSED {
            assert!(
                !may_configure(&h, kind),
                "{kind} should not be configurable"
            );
            assert!(!may_deliver(&h, kind), "{kind} should not deliver");
        }
    }

    #[test]
    fn active_license_allows_every_licensed_kind() {
        let h = handle(LicenseStatus::Active(license_with(
            vec![Feature::Integrations],
            30,
        )));
        for kind in LICENSED {
            assert!(may_configure(&h, kind));
            assert!(may_deliver(&h, kind));
        }
    }

    #[test]
    fn license_without_the_feature_is_locked() {
        let h = handle(LicenseStatus::Active(license_with(
            vec![Feature::Observability],
            30,
        )));
        assert!(!may_configure(&h, IntegrationKind::Slack));
        assert!(!may_deliver(&h, IntegrationKind::Slack));
    }

    // Grace keeps existing alerting alive but refuses new configuration.
    #[test]
    fn grace_delivers_but_refuses_configuration() {
        let h = handle(LicenseStatus::Grace(license_with(
            vec![Feature::Integrations],
            -1,
        )));
        for kind in LICENSED {
            assert!(
                may_deliver(&h, kind),
                "{kind} should still deliver in grace"
            );
            assert!(
                !may_configure(&h, kind),
                "{kind} should not be configurable in grace"
            );
        }
    }

    #[test]
    fn past_grace_stops_delivery() {
        let h = handle(LicenseStatus::Expired(license_with(
            vec![Feature::Integrations],
            -(GRACE_DAYS + 10),
        )));
        for kind in LICENSED {
            assert!(!may_deliver(&h, kind));
        }
    }
}
