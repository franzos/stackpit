//! One-shot banner messages, and the redirect that carries one.
//!
//! Two shapes share one renderer. A handler holding text it cannot enumerate —
//! an SSRF validator's reason, a database error — builds a [`Flash`] and renders
//! in place. A handler that redirects carries a *key* through `?flash=`, which
//! the GET it lands on resolves against the request locale. The key space is
//! closed, and that is what keeps the query parameter from being a way to write
//! arbitrary text onto the page.

use axum::response::{IntoResponse, Redirect, Response};

/// Which of the two banner styles a message wants.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum FlashKind {
    Success,
    Error,
}

#[derive(Clone)]
pub(crate) struct Flash {
    pub(crate) kind: FlashKind,
    pub(crate) text: String,
}

impl Flash {
    pub(crate) fn ok(text: String) -> Self {
        Self {
            kind: FlashKind::Success,
            text,
        }
    }

    pub(crate) fn err(text: String) -> Self {
        Self {
            kind: FlashKind::Error,
            text,
        }
    }

    /// The banner class. Templates call this rather than branching, so every
    /// surface picks the same class for the same kind.
    pub(crate) fn css_class(&self) -> &'static str {
        match self.kind {
            FlashKind::Success => "settings-message",
            FlashKind::Error => "settings-message-error",
        }
    }
}

/// Every key a `?flash=` may carry. The Fluent id is the token prefixed with
/// `flash-`, so adding a message is one row here plus one line of `en`/`de`.
const CATALOGUE: &[(&str, FlashKind)] = &[
    // Issue trackers
    ("tracker-create-failed", FlashKind::Error),
    ("tracker-config-incomplete", FlashKind::Error),
    ("tracker-unlinked", FlashKind::Success),
    ("tracker-ambiguous", FlashKind::Error),
    // Org integrations
    ("integration-license-required", FlashKind::Error),
    ("integration-created", FlashKind::Success),
    ("integration-name-exists", FlashKind::Error),
    ("name-required", FlashKind::Error),
    ("invalid-integration-kind", FlashKind::Error),
    ("invalid-email-provider", FlashKind::Error),
    ("api-token-required", FlashKind::Error),
    ("from-address-required", FlashKind::Error),
    ("email-not-configured", FlashKind::Error),
    ("smtp-not-configured", FlashKind::Error),
    ("invalid-to-address", FlashKind::Error),
    ("global-email-needs-recipient", FlashKind::Error),
    ("url-required", FlashKind::Error),
    ("secret-not-configured", FlashKind::Error),
    ("integration-not-found", FlashKind::Error),
    ("integration-global-not-for-trackers", FlashKind::Error),
    ("integration-saved", FlashKind::Success),
    ("integration-deleted", FlashKind::Success),
    ("integration-no-url", FlashKind::Error),
    ("test-notification-sent", FlashKind::Success),
    ("project-not-found-or-denied", FlashKind::Error),
    ("project-excluded", FlashKind::Success),
    ("project-included", FlashKind::Success),
    // Delivery queue
    ("queue-item-not-found", FlashKind::Error),
    ("queue-replayed", FlashKind::Success),
    ("queue-replay-failed-generic", FlashKind::Error),
    ("queue-cancelled", FlashKind::Success),
    // Projects
    ("project-name-required", FlashKind::Error),
    // Organizations
    ("org-cap-reached", FlashKind::Error),
    // Licence activation
    ("license-activated", FlashKind::Success),
    ("license-deactivated", FlashKind::Success),
    ("license-persist-failed", FlashKind::Error),
    ("license-clear-failed", FlashKind::Error),
    ("license-empty", FlashKind::Error),
    ("license-bad-signature", FlashKind::Error),
    ("license-wrong-product", FlashKind::Error),
    ("license-unreadable", FlashKind::Error),
];

pub(crate) const TRACKER_CREATE_FAILED: &str = "tracker-create-failed";
pub(crate) const TRACKER_CONFIG_INCOMPLETE: &str = "tracker-config-incomplete";
pub(crate) const TRACKER_UNLINKED: &str = "tracker-unlinked";
pub(crate) const TRACKER_AMBIGUOUS: &str = "tracker-ambiguous";
pub(crate) const INTEGRATION_LICENSE_REQUIRED: &str = "integration-license-required";
pub(crate) const INTEGRATION_CREATED: &str = "integration-created";
pub(crate) const INTEGRATION_NAME_EXISTS: &str = "integration-name-exists";
pub(crate) const NAME_REQUIRED: &str = "name-required";
pub(crate) const INVALID_INTEGRATION_KIND: &str = "invalid-integration-kind";
pub(crate) const INVALID_EMAIL_PROVIDER: &str = "invalid-email-provider";
pub(crate) const API_TOKEN_REQUIRED: &str = "api-token-required";
pub(crate) const FROM_ADDRESS_REQUIRED: &str = "from-address-required";
pub(crate) const EMAIL_NOT_CONFIGURED: &str = "email-not-configured";
pub(crate) const SMTP_NOT_CONFIGURED: &str = "smtp-not-configured";
pub(crate) const INVALID_TO_ADDRESS: &str = "invalid-to-address";
pub(crate) const GLOBAL_EMAIL_NEEDS_RECIPIENT: &str = "global-email-needs-recipient";
pub(crate) const URL_REQUIRED: &str = "url-required";
pub(crate) const SECRET_NOT_CONFIGURED: &str = "secret-not-configured";
pub(crate) const INTEGRATION_NOT_FOUND: &str = "integration-not-found";
pub(crate) const INTEGRATION_GLOBAL_NOT_FOR_TRACKERS: &str = "integration-global-not-for-trackers";
pub(crate) const INTEGRATION_SAVED: &str = "integration-saved";
pub(crate) const INTEGRATION_DELETED: &str = "integration-deleted";
pub(crate) const INTEGRATION_NO_URL: &str = "integration-no-url";
pub(crate) const TEST_NOTIFICATION_SENT: &str = "test-notification-sent";
pub(crate) const PROJECT_NOT_FOUND_OR_DENIED: &str = "project-not-found-or-denied";
pub(crate) const PROJECT_EXCLUDED: &str = "project-excluded";
pub(crate) const PROJECT_INCLUDED: &str = "project-included";
pub(crate) const QUEUE_ITEM_NOT_FOUND: &str = "queue-item-not-found";
pub(crate) const QUEUE_REPLAYED: &str = "queue-replayed";
pub(crate) const QUEUE_REPLAY_FAILED: &str = "queue-replay-failed-generic";
pub(crate) const QUEUE_CANCELLED: &str = "queue-cancelled";
pub(crate) const PROJECT_NAME_REQUIRED: &str = "project-name-required";
pub(crate) const LICENSE_ACTIVATED: &str = "license-activated";
pub(crate) const LICENSE_DEACTIVATED: &str = "license-deactivated";
pub(crate) const LICENSE_PERSIST_FAILED: &str = "license-persist-failed";
pub(crate) const LICENSE_CLEAR_FAILED: &str = "license-clear-failed";
pub(crate) const LICENSE_EMPTY: &str = "license-empty";
pub(crate) const LICENSE_BAD_SIGNATURE: &str = "license-bad-signature";
pub(crate) const LICENSE_WRONG_PRODUCT: &str = "license-wrong-product";
pub(crate) const LICENSE_UNREADABLE: &str = "license-unreadable";

fn kind_of(token: &str) -> Option<FlashKind> {
    CATALOGUE.iter().find(|(t, _)| *t == token).map(|(_, k)| *k)
}

/// Resolve a `?flash=` token into a localised banner. An unknown token yields
/// `None`, so nothing outside the catalogue reaches the page.
pub(crate) fn resolve(
    token: Option<&str>,
    locale: &crate::locale::LanguageIdentifier,
) -> Option<Flash> {
    let token = token?;
    let kind = kind_of(token)?;
    let text = crate::i18n::lookup(locale, &format!("flash-{token}"));
    Some(Flash { kind, text })
}

/// Build a banner from a catalogue key for a handler that renders in place
/// rather than redirecting. Severity comes from the same table the redirects
/// read, so a key means the same thing whichever route it takes. An unknown
/// key would be a typo at a call site, so it renders as an error.
pub(crate) fn of(locale: &crate::locale::LanguageIdentifier, token: &str) -> Flash {
    debug_assert!(kind_of(token).is_some(), "unknown flash key: {token}");
    Flash {
        kind: kind_of(token).unwrap_or(FlashKind::Error),
        text: crate::i18n::lookup(locale, &format!("flash-{token}")),
    }
}

/// `303` to `path` carrying one catalogue key. Panics in debug on an unknown
/// key, which is a typo at a call site rather than anything a request controls.
pub(crate) fn redirect(path: &str, token: &str) -> Response {
    debug_assert!(kind_of(token).is_some(), "unknown flash key: {token}");
    let sep = if path.contains('?') { '&' } else { '?' };
    Redirect::to(&format!("{path}{sep}flash={token}")).into_response()
}

/// Pull the `flash` value out of a raw query string.
pub(crate) fn token_from_query(query: &str) -> Option<&str> {
    query
        .split('&')
        .find_map(|kv| kv.strip_prefix("flash="))
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use unic_langid::langid;

    #[test]
    fn an_unknown_key_resolves_to_nothing() {
        assert!(resolve(Some("not-a-key"), &langid!("en")).is_none());
        assert!(resolve(Some("<script>"), &langid!("en")).is_none());
        assert!(resolve(Some(""), &langid!("en")).is_none());
        assert!(resolve(None, &langid!("en")).is_none());
    }

    #[test]
    fn every_catalogue_key_has_a_string_in_en_and_de() {
        for (token, _) in CATALOGUE {
            for locale in [langid!("en"), langid!("de")] {
                let f = resolve(Some(token), &locale).expect("catalogue key resolves");
                assert!(
                    !f.text.contains(crate::i18n::MISSING_PREFIX),
                    "flash-{token} missing in {locale}"
                );
            }
        }
    }

    #[test]
    fn kinds_pick_their_class() {
        let e = resolve(Some(URL_REQUIRED), &langid!("en")).unwrap();
        assert_eq!(e.css_class(), "settings-message-error");
        let s = resolve(Some(QUEUE_CANCELLED), &langid!("en")).unwrap();
        assert_eq!(s.css_class(), "settings-message");
    }

    #[test]
    fn redirect_appends_to_an_existing_query() {
        let with = redirect("/web/x?tab=details", TRACKER_UNLINKED);
        let bare = redirect("/web/x", TRACKER_UNLINKED);
        let loc = |r: &Response| {
            r.headers()
                .get(axum::http::header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(loc(&with), "/web/x?tab=details&flash=tracker-unlinked");
        assert_eq!(loc(&bare), "/web/x?flash=tracker-unlinked");
    }

    #[test]
    fn token_is_read_from_any_position_in_the_query() {
        assert_eq!(
            token_from_query("flash=queue-replayed"),
            Some("queue-replayed")
        );
        assert_eq!(
            token_from_query("tab=details&flash=tracker-unlinked"),
            Some("tracker-unlinked")
        );
        assert_eq!(token_from_query("lang=de"), None);
        assert_eq!(token_from_query("flash="), None);
    }
}
