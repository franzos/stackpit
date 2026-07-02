//! Process-wide Fluent loader + raw lookups. Bidi isolation is disabled;
//! RTL isolation is done in HTML with <bdi> (P1b).
use crate::locale::LanguageIdentifier;
use fluent_templates::fluent_bundle::FluentValue;
use fluent_templates::{static_loader, Loader};
use std::borrow::Cow;
use std::collections::HashMap;

#[cfg(test)]
pub(crate) const MISSING_PREFIX: &str = "Unknown localization key:";

static_loader! {
    static LOADER = {
        locales: "./locales",
        fallback_language: "en",
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

pub(crate) fn lookup(lang: &LanguageIdentifier, id: &str) -> String {
    LOADER.lookup(lang, id)
}

pub(crate) fn lookup_args(
    lang: &LanguageIdentifier,
    id: &str,
    args: &HashMap<Cow<'static, str>, FluentValue>,
) -> String {
    LOADER.lookup_with_args(lang, id, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use unic_langid::langid;

    #[test]
    fn looks_up_seed_key_per_locale() {
        assert_eq!(lookup(&langid!("en"), "common-action-save"), "Save");
        assert_eq!(lookup(&langid!("de"), "common-action-save"), "Speichern");
    }

    #[test]
    fn missing_key_returns_placeholder_not_panic() {
        let out = lookup(&langid!("en"), "does-not-exist");
        assert!(out.contains(MISSING_PREFIX), "got: {out}");
    }

    #[test]
    fn de_seed_hit() {
        assert_eq!(lookup(&langid!("de"), "nav-logout"), "Abmelden");
    }
}
