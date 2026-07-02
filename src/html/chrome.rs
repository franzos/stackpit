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

impl PageChrome {
    pub(crate) fn new(csrf_token: String, locale: LanguageIdentifier, path: String) -> Self {
        Self {
            csrf_token,
            locale,
            path,
        }
    }

    pub(crate) fn t(&self, id: &str) -> String {
        crate::i18n::lookup(&self.locale, id)
    }

    pub(crate) fn err(&self, e: impl std::fmt::Display) -> String {
        format!("{} {e}", self.t("common-error-prefix"))
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
        if delta < 60 {
            return self.t("common-time-just-now");
        }
        let secs = delta as u64;
        let tn = |id: &str, n: u64| {
            let mut a: HashMap<Cow<'static, str>, FluentValue> = HashMap::new();
            a.insert(Cow::Borrowed("n"), (n as i64).into());
            crate::i18n::lookup_args(&self.locale, id, &a)
        };
        if secs < 3600 {
            tn("common-time-min-ago", secs / 60)
        } else if secs < 86400 {
            tn("common-time-hour-ago", secs / 3600)
        } else if secs < 604_800 {
            tn("common-time-day-ago", secs / 86400)
        } else {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| ts.to_string())
        }
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
}
