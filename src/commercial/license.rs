//! License data model — what Stackpit carries in memory after
//! [`crate::commercial::verify::decode_and_verify`] has accepted a blob.
//!
//! The on-wire `Claims` shape (in the `license-issuer` repo) is deliberately
//! stringly-typed for forward compatibility; this module normalises it into
//! typed enums so the rest of Stackpit pattern-matches against [`Feature`]
//! instead of stringly-typed claims.

use chrono::{DateTime, Utc};

/// Gated feature names recognised by Stackpit. The license blob carries
/// feature strings, and each variant here maps to one of them via
/// [`Feature::wire_name`]; unrecognised strings are ignored at parse time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    Observability,
    /// Slack, generic webhooks, and the GitHub/Forgejo/GitLab issue trackers.
    /// Email is deliberately outside this — it's the free baseline channel.
    Integrations,
}

impl Feature {
    pub fn wire_name(self) -> &'static str {
        match self {
            Feature::Observability => "observability",
            Feature::Integrations => "integrations",
        }
    }
    /// Reverse of [`Feature::wire_name`].
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "observability" => Some(Feature::Observability),
            "integrations" => Some(Feature::Integrations),
            _ => None,
        }
    }
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Feature::Observability => "Observability",
            Feature::Integrations => "Integrations",
        }
    }
}

/// True when `current` is strictly below `cap`. `None` (unlimited) is
/// always under.
#[allow(dead_code)]
pub fn org_cap_allows(cap: Option<u32>, current: u32) -> bool {
    cap.is_none_or(|c| current < c)
}

/// Normalised, post-verification license. Held by [`LicenseStatus`] —
/// constructed by [`crate::commercial::verify::decode_and_verify`].
#[derive(Debug, Clone)]
pub struct License {
    pub license_id: String,
    pub customer: String,
    pub email: String,
    pub issued_at: DateTime<Utc>,
    /// `None` = lifetime license.
    pub expires_at: Option<DateTime<Utc>>,
    pub features: Vec<Feature>,
    /// `None` = unlimited.
    pub max_orgs: Option<u32>,
    /// The SKU the licence was sold as (`pro`, `business`, …). Read from the
    /// blob rather than assumed: the persisted row hardcoded `"business"`, so
    /// every licence looked like the top tier.
    pub tier: String,
    /// Which product the blob is for. Verification already rejects a foreign
    /// product; this is what the row records.
    pub product: String,
}

impl License {
    pub fn has_feature(&self, feature: Feature) -> bool {
        self.features.contains(&feature)
    }

    /// True iff a hard expiry has passed (lifetime licenses never expire).
    pub fn is_past_expiry(&self, now: DateTime<Utc>) -> bool {
        match self.expires_at {
            None => false,
            Some(exp) => now > exp,
        }
    }
}

/// Cached runtime status. Active / Grace / Expired / Unlicensed lets the
/// dashboard banner decide what to render without re-evaluating clock math
/// on every request.
#[derive(Debug, Clone)]
pub enum LicenseStatus {
    /// No license row in the DB. OSS-tier shape.
    Unlicensed,
    /// Active license, before any expiry.
    Active(License),
    /// Past hard expiry but inside the configured grace window. Features
    /// gated by the license go read-only.
    Grace(License),
    /// Past expiry AND past the grace window. Treated like Unlicensed for
    /// feature checks, but the dashboard surfaces a "Renew" banner.
    Expired(License),
}

impl LicenseStatus {
    pub fn license(&self) -> Option<&License> {
        match self {
            LicenseStatus::Unlicensed => None,
            LicenseStatus::Active(l) | LicenseStatus::Grace(l) | LicenseStatus::Expired(l) => {
                Some(l)
            }
        }
    }
}

/// What the gate at a call site sees.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum FeatureStatus {
    /// Fully licensed — proceed with the real action.
    Allowed,
    /// Inside the grace window — surface the feature as read-only and
    /// nudge the operator to renew. Hard POSTs (e.g. "create new org")
    /// MUST still bail; reads stay accessible.
    GraceReadOnly,
    /// Not licensed (no blob, blob missing the feature, or past grace).
    /// Render the upsell page.
    Locked,
}

/// Pure function so it can be unit-tested without an `ArcSwap`.
pub(crate) fn evaluate_feature(status: &LicenseStatus, feature: Feature) -> FeatureStatus {
    let license = match status {
        LicenseStatus::Unlicensed | LicenseStatus::Expired(_) => return FeatureStatus::Locked,
        LicenseStatus::Active(l) | LicenseStatus::Grace(l) => l,
    };
    if !license.has_feature(feature) {
        return FeatureStatus::Locked;
    }
    match status {
        LicenseStatus::Active(_) => FeatureStatus::Allowed,
        LicenseStatus::Grace(_) => FeatureStatus::GraceReadOnly,
        // Unreachable — the two non-licensed arms returned `Locked` above.
        _ => FeatureStatus::Locked,
    }
}

/// Fixed read-only window after license expiry before hard-gating; not operator-configurable.
pub const GRACE_DAYS: i64 = 30;

/// Reclassify an `Active` license against the wall clock. Called at boot
/// (after the DB row is decoded) and on every activation. Keeps the
/// runtime status in sync with reality without forcing every feature
/// check to redo the expiry math.
pub fn classify(license: License, grace_days: i64, now: DateTime<Utc>) -> LicenseStatus {
    if !license.is_past_expiry(now) {
        return LicenseStatus::Active(license);
    }
    let days_past = license
        .expires_at
        .map(|exp| (now - exp).num_days())
        .unwrap_or(0);
    if days_past <= grace_days {
        LicenseStatus::Grace(license)
    } else {
        LicenseStatus::Expired(license)
    }
}

/// Stable label for a status variant, used both for logging and for
/// cheap change-detection in the periodic re-classification task.
pub(crate) fn status_variant(status: &LicenseStatus) -> &'static str {
    match status {
        LicenseStatus::Unlicensed => "Unlicensed",
        LicenseStatus::Active(_) => "Active",
        LicenseStatus::Grace(_) => "Grace",
        LicenseStatus::Expired(_) => "Expired",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn license_with(expires: Option<DateTime<Utc>>) -> License {
        License {
            license_id: "test".into(),
            customer: "Test Co".into(),
            email: "t@example.com".into(),
            issued_at: Utc::now(),
            expires_at: expires,
            features: Vec::new(),
            max_orgs: None,
            tier: "business".into(),
            product: "stackpit".into(),
        }
    }

    #[test]
    fn classify_picks_grace_under_window() {
        let l = license_with(Some(Utc::now() - chrono::Duration::days(3)));
        let s = classify(l, 14, Utc::now());
        assert!(matches!(s, LicenseStatus::Grace(_)));
    }

    #[test]
    fn classify_picks_expired_past_window() {
        let l = license_with(Some(Utc::now() - chrono::Duration::days(30)));
        let s = classify(l, 14, Utc::now());
        assert!(matches!(s, LicenseStatus::Expired(_)));
    }

    #[test]
    fn grace_window_is_fixed_at_thirty_days() {
        assert_eq!(GRACE_DAYS, 30);

        let in_grace = license_with(Some(Utc::now() - chrono::Duration::days(20)));
        assert!(matches!(
            classify(in_grace, GRACE_DAYS, Utc::now()),
            LicenseStatus::Grace(_)
        ));

        let past_grace = license_with(Some(Utc::now() - chrono::Duration::days(40)));
        assert!(matches!(
            classify(past_grace, GRACE_DAYS, Utc::now()),
            LicenseStatus::Expired(_)
        ));
    }

    fn licensed(features: Vec<Feature>) -> LicenseStatus {
        LicenseStatus::Active(License {
            license_id: "test".into(),
            customer: "T".into(),
            email: "t@e.test".into(),
            issued_at: Utc::now(),
            expires_at: Some(Utc::now() + Duration::days(30)),
            features,
            max_orgs: None,
            tier: "business".into(),
            product: "stackpit".into(),
        })
    }

    #[test]
    fn observability_active_is_allowed() {
        assert!(matches!(
            evaluate_feature(
                &licensed(vec![Feature::Observability]),
                Feature::Observability
            ),
            FeatureStatus::Allowed
        ));
    }

    #[test]
    fn observability_absent_is_locked() {
        assert!(matches!(
            evaluate_feature(&licensed(vec![]), Feature::Observability),
            FeatureStatus::Locked
        ));
    }

    #[test]
    fn observability_wire_roundtrips() {
        assert_eq!(Feature::Observability.wire_name(), "observability");
        assert_eq!(
            Feature::from_wire("observability"),
            Some(Feature::Observability)
        );
    }
}
