use std::sync::LazyLock;

use axum::http::HeaderMap;
use unic_langid::langid;
pub(crate) use unic_langid::LanguageIdentifier;

pub(crate) const LOCALE_COOKIE: &str = "sp_locale";

/// Supported UI locales as (BCP-47 code, endonym). The endonym is the language's
/// own name, shown untranslated in the switcher. Adding a locale is one row here
/// plus a matching `locales/<code>/` catalog; the switcher and negotiation both
/// read from this list.
pub(crate) const LOCALES: &[(&str, &str)] = &[
    ("en", "English"),
    ("de", "Deutsch"),
    ("fr", "Français"),
    ("es", "Español"),
    ("it", "Italiano"),
    ("pt", "Português"),
    ("ru", "Русский"),
    ("th", "ไทย"),
    ("ar", "العربية"),
];

/// Codes from `LOCALES` parsed once; negotiate/accept run per request.
static SUPPORTED_LANGS: LazyLock<Vec<LanguageIdentifier>> = LazyLock::new(|| {
    LOCALES
        .iter()
        .map(|(code, _)| code.parse().expect("LOCALES code valid"))
        .collect()
});

pub(crate) fn default_locale() -> LanguageIdentifier {
    langid!("en")
}

fn supported() -> &'static [LanguageIdentifier] {
    &SUPPORTED_LANGS
}

pub(crate) fn negotiate(requested: &[LanguageIdentifier]) -> LanguageIdentifier {
    let available = supported();
    let default = default_locale();
    let matched = fluent_langneg::negotiate_languages(
        requested,
        available,
        Some(&default),
        fluent_langneg::NegotiationStrategy::Filtering,
    );
    matched.first().map(|l| (*l).clone()).unwrap_or(default)
}

/// The single validating parser: accept a tag only if it negotiates to a
/// SUPPORTED locale. Reused by query, cookie, and the OIDC locale claim.
pub(crate) fn accept(value: &str) -> Option<LanguageIdentifier> {
    let parsed: LanguageIdentifier = value.parse().ok()?;
    if LOCALES
        .iter()
        .any(|(code, _)| *code == parsed.language.as_str())
    {
        Some(negotiate(&[parsed]))
    } else {
        None
    }
}

pub(crate) fn from_accept_language(header: Option<&str>) -> LanguageIdentifier {
    let Some(raw) = header else {
        return default_locale();
    };
    let requested: Vec<LanguageIdentifier> = raw
        .split(',')
        .filter_map(|part| part.split(';').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    negotiate(&requested)
}

pub(crate) fn read_locale_cookie(headers: &HeaderMap) -> Option<LanguageIdentifier> {
    crate::middleware::cookie::read_cookie(headers, LOCALE_COOKIE).and_then(accept)
}

/// Builds the `sp_locale` Set-Cookie value. `code` must be a canonical
/// accepted locale (never a raw path/form value). Scoped to `/web` and
/// HttpOnly; `Secure` follows the deployment's TLS posture.
pub(crate) fn locale_cookie(code: &str, secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    format!("{LOCALE_COOKIE}={code}; Path=/web; HttpOnly; SameSite=Strict; Max-Age=31536000{secure_flag}")
}

pub(crate) fn dir_for(lang: &LanguageIdentifier) -> &'static str {
    match lang.language.as_str() {
        "ar" | "he" | "fa" | "ur" => "rtl",
        _ => "ltr",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unic_langid::langid;

    #[test]
    fn negotiate_prefers_supported_else_default() {
        assert_eq!(negotiate(&[langid!("de-AT")]), langid!("de"));
        assert_eq!(negotiate(&[langid!("fr-CA")]), langid!("fr"));
        assert_eq!(negotiate(&[langid!("ja")]), langid!("en"));
        assert_eq!(negotiate(&[]), langid!("en"));
    }

    #[test]
    fn accept_only_supported() {
        assert_eq!(accept("de"), Some(langid!("de")));
        assert_eq!(accept("de-AT"), Some(langid!("de")));
        assert_eq!(accept("ar"), Some(langid!("ar")));
        assert_eq!(accept("ja"), None);
        assert_eq!(accept("garbage!"), None);
    }

    // Gate for the untrusted OIDC `locale` claim: region tag narrows to `de`,
    // unsupported/junk provider values reject.
    #[test]
    fn accept_gates_oidc_locale_claim() {
        assert_eq!(accept("de-DE"), Some(langid!("de")));
        assert_eq!(accept("zz-nonsense"), None);
        assert_eq!(accept("ja-JP"), None);
    }

    #[test]
    fn from_accept_language_picks_best() {
        assert_eq!(
            from_accept_language(Some("de-DE,de;q=0.9,en;q=0.8")),
            langid!("de")
        );
        assert_eq!(from_accept_language(None), langid!("en"));
    }

    #[test]
    fn dir_for_rtl_and_ltr() {
        assert_eq!(dir_for(&langid!("ar")), "rtl");
        assert_eq!(dir_for(&langid!("en")), "ltr");
    }
}
