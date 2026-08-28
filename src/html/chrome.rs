use std::borrow::Cow;
use std::collections::HashMap;

use fluent_templates::fluent_bundle::FluentValue;

use crate::locale::LanguageIdentifier;

/// Shared page-shell data threaded into every base-extending template.
/// Carries the CSRF token plus the resolved request locale; the `t*` helpers
/// look strings up against that locale (rendered flip lands in a later task).
#[derive(Clone)]
pub(crate) struct PageChrome {
    pub(crate) csrf_token: String,
    pub(crate) locale: LanguageIdentifier,
    // Current request path+query, so the in-app switcher can build `next` server-side.
    pub(crate) path: String,
    // "customer · email" when a commercial license is active AND the request is on an
    // admin surface; None everywhere else. Feeds the sidebar watermark.
    pub(crate) license_watermark: Option<String>,
    // Name of the org the request is scoped to. The active org is a mode, so it needs a
    // persistent indicator; None on the admin-token and loopback paths, which have no name.
    pub(crate) active_org: Option<String>,
    // Banner carried here from a `?flash=` key, already resolved against `locale`.
    // A page that builds its own message renders that instead; see the `flash` macro.
    pub(crate) flash: Option<crate::html::flash::Flash>,
}

/// One entry in the language switcher: BCP-47 code, endonym (shown untranslated),
/// and whether it is the active request locale.
pub(crate) struct LocaleChoice {
    pub(crate) code: &'static str,
    pub(crate) name: &'static str,
    pub(crate) active: bool,
}

fn locale_choices(active: &LanguageIdentifier) -> Vec<LocaleChoice> {
    crate::locale::LOCALES
        .iter()
        .map(|(code, name)| LocaleChoice {
            code,
            name,
            active: active.language.as_str() == *code,
        })
        .collect()
}

fn locale_endonym(active: &LanguageIdentifier) -> &'static str {
    crate::locale::LOCALES
        .iter()
        .find(|(code, _)| active.language.as_str() == *code)
        .map(|(_, name)| *name)
        .unwrap_or("English")
}

// Tier boundaries, shared by the past and future relative-time ladders. The
// two ladders differ in depth and floor, so only these are common.
const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const WEEK: i64 = 7 * DAY;
const MONTH: i64 = 30 * DAY;
const YEAR: i64 = 365 * DAY;

/// Paths where the commercial licensee watermark is surfaced.
fn is_admin_surface(path: &str) -> bool {
    path.starts_with("/web/admin")
        || path.starts_with("/web/settings")
        || path.starts_with("/web/organizations")
}

impl PageChrome {
    pub(crate) fn new(csrf_token: String, locale: LanguageIdentifier, path: String) -> Self {
        Self {
            csrf_token,
            locale,
            path,
            license_watermark: None,
            active_org: None,
            flash: None,
        }
    }

    /// Resolves a `?flash=` token against the request locale. An unknown token
    /// leaves the banner absent.
    pub(crate) fn with_flash(mut self, token: Option<&str>) -> Self {
        self.flash = crate::html::flash::resolve(token, &self.locale);
        self
    }

    /// Sets the scope indicator to the active org's name.
    pub(crate) fn with_active_org(mut self, name: Option<String>) -> Self {
        self.active_org = name;
        self
    }

    /// Sets the sidebar watermark to "customer · email" when a license is
    /// present and the request path is an admin surface; a no-op otherwise.
    pub(crate) fn with_license_watermark(
        mut self,
        status: &crate::commercial::LicenseStatus,
    ) -> Self {
        if let Some(l) = status.license() {
            if is_admin_surface(&self.path) {
                self.license_watermark = Some(format!("{} · {}", l.customer, l.email));
            }
        }
        self
    }

    pub(crate) fn t(&self, id: &str) -> String {
        crate::i18n::lookup(&self.locale, id)
    }

    /// A banner for a catalogue key, localised and with the catalogue's severity.
    pub(crate) fn flash_of(&self, token: &str) -> crate::html::flash::Flash {
        crate::html::flash::of(&self.locale, token)
    }

    /// An error banner for text this side computed — a database error, a
    /// validator's reason — which no catalogue key can carry.
    pub(crate) fn flash_err(&self, e: impl Into<anyhow::Error>) -> crate::html::flash::Flash {
        crate::html::flash::Flash::err(self.err(e))
    }

    pub(crate) fn err(&self, e: impl Into<anyhow::Error>) -> String {
        format!(
            "{} {}",
            self.t("common-error-prefix"),
            crate::html::safe_error_message(&e.into())
        )
    }

    // Borrow<i64> so askama call sites (which pass `&(x as i64)`) and plain i64
    // literals both work without a cast dance.
    pub(crate) fn tv_count(&self, id: &str, count: impl std::borrow::Borrow<i64>) -> String {
        let mut a: HashMap<Cow<'static, str>, FluentValue> = HashMap::new();
        a.insert(Cow::Borrowed("count"), (*count.borrow()).into());
        crate::i18n::lookup_args(&self.locale, id, &a)
    }

    pub(crate) fn rel_time(&self, ts: impl std::borrow::Borrow<i64>) -> String {
        let ts = *ts.borrow();
        let delta = chrono::Utc::now().timestamp() - ts;
        if delta < MINUTE {
            return self.t("common-time-just-now");
        }
        let secs = delta as u64;
        if secs < HOUR as u64 {
            self.tn("common-time-min-ago", secs / MINUTE as u64)
        } else if secs < DAY as u64 {
            self.tn("common-time-hour-ago", secs / HOUR as u64)
        } else if secs < WEEK as u64 {
            self.tn("common-time-day-ago", secs / DAY as u64)
        } else if secs < MONTH as u64 {
            self.tn("common-time-week-ago", secs / WEEK as u64)
        } else if secs < YEAR as u64 {
            self.tn("common-time-month-ago", secs / MONTH as u64)
        } else {
            self.tn("common-time-year-ago", secs / YEAR as u64)
        }
    }

    /// The forward-looking twin of [`rel_time`](Self::rel_time): "in 30s",
    /// "in 2h". A timestamp already in the past delegates to `rel_time`, so a
    /// due item reads "just now" and no caller has to branch.
    ///
    /// The ladder stops at days on purpose. The only user is the delivery
    /// queue's next attempt, which the backoff caps at an hour and the
    /// giving-up window at 24. The two ladders share their boundaries and
    /// nothing else — `rel_time` also has a "just now" floor and six tiers, so
    /// one helper parameterised by key family would not cover both.
    pub(crate) fn rel_time_future(&self, ts: impl std::borrow::Borrow<i64>) -> String {
        let ts = *ts.borrow();
        let delta = ts - chrono::Utc::now().timestamp();
        if delta <= 0 {
            return self.rel_time(ts);
        }
        let secs = delta as u64;
        if secs < MINUTE as u64 {
            self.tn("common-time-in-secs", secs)
        } else if secs < HOUR as u64 {
            self.tn("common-time-in-min", secs / MINUTE as u64)
        } else if secs < DAY as u64 {
            self.tn("common-time-in-hour", secs / HOUR as u64)
        } else {
            self.tn("common-time-in-day", secs / DAY as u64)
        }
    }

    fn tn(&self, id: &str, n: u64) -> String {
        let mut a: HashMap<Cow<'static, str>, FluentValue> = HashMap::new();
        a.insert(Cow::Borrowed("n"), (n as i64).into());
        crate::i18n::lookup_args(&self.locale, id, &a)
    }

    pub(crate) fn tv1(&self, id: &str, name: &str, val: &str) -> String {
        let mut a: HashMap<Cow<'static, str>, FluentValue> = HashMap::new();
        a.insert(Cow::Owned(name.to_string()), val.to_string().into());
        crate::i18n::lookup_args(&self.locale, id, &a)
    }

    pub(crate) fn dir(&self) -> &'static str {
        crate::locale::dir_for(&self.locale)
    }

    /// All supported locales for the switcher dropdown.
    pub(crate) fn locales(&self) -> Vec<LocaleChoice> {
        locale_choices(&self.locale)
    }

    /// Endonym of the active locale (the switcher's summary label).
    pub(crate) fn locale_name(&self) -> &'static str {
        locale_endonym(&self.locale)
    }
}

/// Shared locale helpers for the standalone (non-`PageChrome`) templates that
/// carry their own `locale` field. `PageChrome` keeps its own inherent copies
/// so the ~28 base-extending template modules resolve `chrome.t(...)` without
/// needing this trait in scope.
pub(crate) trait Localized {
    fn locale(&self) -> &LanguageIdentifier;

    fn t(&self, id: &str) -> String {
        crate::i18n::lookup(self.locale(), id)
    }

    fn dir(&self) -> &'static str {
        crate::locale::dir_for(self.locale())
    }

    fn locales(&self) -> Vec<LocaleChoice> {
        locale_choices(self.locale())
    }

    fn locale_name(&self) -> &'static str {
        locale_endonym(self.locale())
    }
}

/// Resolve the request locale via the ladder:
/// `?lang=` query > `sp_locale` cookie > persisted `preferred` > `Accept-Language` > `en`.
pub(crate) fn resolve_locale(
    parts: &axum::http::request::Parts,
    preferred: Option<&str>,
) -> LanguageIdentifier {
    if let Some(q) = parts.uri.query() {
        if let Some(l) = q
            .split('&')
            .find_map(|kv| kv.strip_prefix("lang="))
            .and_then(crate::locale::accept)
        {
            return l;
        }
    }
    if let Some(l) = crate::locale::read_locale_cookie(&parts.headers) {
        return l;
    }
    if let Some(l) = preferred.and_then(crate::locale::accept) {
        return l;
    }
    let al = parts
        .headers
        .get(axum::http::header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok());
    crate::locale::from_accept_language(al)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::default_locale;
    use axum::http::Request;
    use unic_langid::langid;

    fn chrome_for(locale: LanguageIdentifier) -> PageChrome {
        PageChrome::new("csrf".to_string(), locale, "/web/projects/".to_string())
    }

    #[test]
    fn t_resolves_per_locale() {
        assert_eq!(chrome_for(langid!("en")).t("common-action-save"), "Save");
        assert_eq!(
            chrome_for(langid!("de")).t("common-action-save"),
            "Speichern"
        );
    }

    #[test]
    fn tv_count_pluralizes() {
        let en = chrome_for(langid!("en"));
        assert_eq!(en.tv_count("test-count", 1), "1 item");
        assert_eq!(en.tv_count("test-count", 3), "3 items");
    }

    #[test]
    fn rel_time_future_climbs_its_ladder_and_falls_back_to_the_past() {
        let c = chrome_for(langid!("en"));
        let now = chrono::Utc::now().timestamp();

        // Already due, or overdue: the caller must not have to branch.
        assert_eq!(c.rel_time_future(now - 1), c.t("common-time-just-now"));
        assert_eq!(c.rel_time_future(now), c.t("common-time-just-now"));

        assert_eq!(c.rel_time_future(now + 30), "in 30s");
        assert_eq!(c.rel_time_future(now + 90), "in 1m");
        assert_eq!(c.rel_time_future(now + 7_200), "in 2h");
        assert_eq!(c.rel_time_future(now + 200_000), "in 2d");
    }

    /// `rel_time` has ~100 call sites and its own tests; the future twin must
    /// not have changed it.
    #[test]
    fn rel_time_is_unchanged_by_its_future_twin() {
        let c = chrome_for(langid!("en"));
        let now = chrono::Utc::now().timestamp();
        assert_eq!(c.rel_time(now - 30), "just now");
        assert_eq!(c.rel_time(now - 120), "2m ago");
        assert_eq!(c.rel_time(now - 7_200), "2h ago");
        assert_eq!(c.rel_time(now - 200_000), "2d ago");
        assert_eq!(c.rel_time(now - 1_300_000), "2w ago");
        assert_eq!(c.rel_time(now - 5_200_000), "2mo ago");
        assert_eq!(c.rel_time(now - 64_000_000), "2y ago");
    }

    #[test]
    fn dir_is_ltr_for_en() {
        assert_eq!(chrome_for(default_locale()).dir(), "ltr");
    }

    fn parts_from(uri: &str, headers: &[(&str, &str)]) -> axum::http::request::Parts {
        let mut builder = Request::builder().uri(uri);
        for (k, v) in headers {
            builder = builder.header(*k, *v);
        }
        builder.body(()).unwrap().into_parts().0
    }

    #[test]
    fn resolve_locale_query_wins() {
        let parts = parts_from(
            "/web/projects/?lang=de",
            &[("cookie", "sp_locale=en"), ("accept-language", "en")],
        );
        assert_eq!(resolve_locale(&parts, Some("en")), langid!("de"));
    }

    #[test]
    fn resolve_locale_cookie_next() {
        let parts = parts_from(
            "/web/projects/",
            &[("cookie", "sp_locale=de"), ("accept-language", "en")],
        );
        assert_eq!(resolve_locale(&parts, Some("en")), langid!("de"));
    }

    #[test]
    fn resolve_locale_preferred_next() {
        let parts = parts_from("/web/projects/", &[("accept-language", "en")]);
        assert_eq!(resolve_locale(&parts, Some("de")), langid!("de"));
    }

    #[test]
    fn resolve_locale_accept_language_next() {
        let parts = parts_from("/web/projects/", &[("accept-language", "de-DE,de;q=0.9")]);
        assert_eq!(resolve_locale(&parts, None), langid!("de"));
    }

    #[test]
    fn resolve_locale_defaults_to_en() {
        let parts = parts_from("/web/projects/", &[]);
        assert_eq!(resolve_locale(&parts, None), langid!("en"));
    }

    fn active_status() -> crate::commercial::LicenseStatus {
        use crate::commercial::license::License;
        crate::commercial::LicenseStatus::Active(License {
            license_id: "test".into(),
            customer: "Cust".into(),
            email: "e@x".into(),
            issued_at: chrono::Utc::now(),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::days(30)),
            features: Vec::new(),
            max_orgs: None,
            tier: "business".into(),
            product: "stackpit".into(),
        })
    }

    fn chrome_at(path: &str) -> PageChrome {
        PageChrome::new("csrf".to_string(), langid!("en"), path.to_string())
    }

    #[test]
    fn watermark_on_admin_surface_with_license() {
        let c = chrome_at("/web/admin/license").with_license_watermark(&active_status());
        assert_eq!(c.license_watermark.as_deref(), Some("Cust · e@x"));
    }

    #[test]
    fn watermark_absent_off_admin_surface() {
        let c = chrome_at("/web/projects/").with_license_watermark(&active_status());
        assert_eq!(c.license_watermark, None);
    }

    #[test]
    fn watermark_absent_when_unlicensed() {
        let c = chrome_at("/web/admin/license")
            .with_license_watermark(&crate::commercial::LicenseStatus::Unlicensed);
        assert_eq!(c.license_watermark, None);
    }
}
