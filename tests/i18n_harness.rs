//! .ftl key-parity harness. Subset direction: every translated key must exist
//! in `en` (catches orphans/typos per locale). `en` is allowed to hold keys a
//! locale has not translated yet (per-key English fallback is by design).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use fluent_syntax::ast::Entry;
use fluent_syntax::parser::parse;

/// Collects message ids and `message.attribute` pairs from every `.ftl` under
/// `locales/<lang>`. Panics on any `Entry::Junk` (malformed Fluent). Terms and
/// comments are skipped.
fn collect_keys(lang: &str) -> BTreeSet<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("locales")
        .join(lang);
    let mut keys = BTreeSet::new();

    let entries = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read locales/{lang}: {e}"))
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().map(|x| x == "ftl").unwrap_or(false));

    for file in entries {
        let path = file.path();
        let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        // parse returns the Resource even on error (Junk entries preserved).
        let resource = match parse(src.as_str()) {
            Ok(r) => r,
            Err((r, _errs)) => r,
        };
        for entry in &resource.body {
            match entry {
                Entry::Message(msg) => {
                    if msg.value.is_some() {
                        keys.insert(msg.id.name.to_string());
                    }
                    for attr in &msg.attributes {
                        keys.insert(format!("{}.{}", msg.id.name, attr.id.name));
                    }
                }
                Entry::Junk { content } => {
                    panic!("malformed Fluent in {path:?} (Entry::Junk): {content}");
                }
                // Terms and comments are not user-facing keys.
                _ => {}
            }
        }
    }

    keys
}

/// Every `locales/<lang>` directory except `en`, sorted for stable output.
fn translated_locales() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
    let mut langs: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read locales: {e}"))
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name != "en")
        .collect();
    langs.sort();
    langs
}

#[test]
fn translated_keys_are_subset_of_en() {
    let en = collect_keys("en");

    for lang in translated_locales() {
        let keys = collect_keys(&lang);
        let orphans: Vec<&String> = keys.difference(&en).collect();
        assert!(
            orphans.is_empty(),
            "{lang} keys missing from en (orphans/typos): {orphans:?}"
        );
    }
}

#[test]
fn coverage_report() {
    // Non-gating: reports per-locale translation coverage without failing CI.
    let en = collect_keys("en");
    let total = en.len();

    for lang in translated_locales() {
        let keys = collect_keys(&lang);
        let translated = en.intersection(&keys).count();
        let ratio = if total == 0 {
            0.0
        } else {
            (translated as f64 / total as f64) * 100.0
        };

        let untranslated: Vec<&String> = en.difference(&keys).take(10).collect();
        println!("i18n {lang} coverage: {translated}/{total} keys ({ratio:.1}%)");
        if !untranslated.is_empty() {
            println!("first untranslated en keys ({lang}): {untranslated:?}");
        }
    }
}
