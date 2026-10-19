#![no_main]

use libfuzzer_sys::fuzz_target;
use stackpit::fuzz_api::{accept, default_locale, from_accept_language};
use stackpit::util::version::version_sort_key;

fuzz_target!(|input: (&str, &str)| {
    let (a, b) = input;
    if let Some(lang) = accept(a) {
        // Accepted tags always land on a supported language.
        assert!(accept(lang.language.as_str()).is_some());
    }
    let _ = from_accept_language(Some(a));
    assert_eq!(from_accept_language(None), default_locale());

    // The sort key must give a consistent total order.
    let ka = version_sort_key(a);
    let kb = version_sort_key(b);
    assert_eq!(ka.cmp(&kb), kb.cmp(&ka).reverse());
    assert_eq!(ka, version_sort_key(a));
});
