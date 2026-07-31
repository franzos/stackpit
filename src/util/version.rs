//! Natural-order sort keys for release versions.

/// Digit runs are zero-padded to this width so lexicographic byte order matches
/// numeric order. Wide enough for build numbers and epoch-style versions.
const DIGIT_WIDTH: usize = 12;

/// Separates the core / pre-release / build sections of a key. Sorts below
/// every character a section can contain, so a section boundary always wins
/// over the content that follows it.
const SEP: char = '\u{1}';

/// Sort keys are persisted per release row; cap them so a pathological version
/// string can't bloat the table.
const MAX_KEY_LEN: usize = 400;

/// Build a key whose lexicographic byte order matches the natural ("human")
/// order of the version.
///
/// Digit runs are zero-padded so `1.0.9` sorts below `1.0.12`, and text is
/// lowercased so `V2` and `v2` land together. The version is split into three
/// sections — core, pre-release (`-suffix` following a digit), and build
/// metadata (everything after the first `+`) — so semver precedence holds:
/// `2.0.0` outranks `2.0.0-rc.1`, which outranks `2.0.0-beta.1`.
///
/// This is a natural sort rather than a real semver parse, so it also produces
/// a stable order for versions semver can't describe (VCS hashes, date stamps,
/// Sentry-style `package@1.2.3` names, which order by package first).
pub fn version_sort_key(version: &str) -> String {
    let (head, build) = match version.split_once('+') {
        Some((head, build)) => (head, Some(build)),
        None => (version, None),
    };
    let (core, prerelease) = split_prerelease(head);

    let mut key = natural_key(core);
    key.push(SEP);
    // A version with no pre-release outranks the same core with one, so the
    // flag has to sort above anything the pre-release branch can emit.
    match prerelease {
        Some(pre) => {
            key.push('0');
            key.push_str(&natural_key(pre));
        }
        None => key.push('1'),
    }
    key.push(SEP);
    if let Some(build) = build {
        key.push_str(&natural_key(build));
    }

    truncate_on_boundary(key)
}

/// Split `1.2.3-rc.1` into `("1.2.3", Some("rc.1"))`. Only a hyphen directly
/// after a digit starts a pre-release, so hyphenated package names such as
/// `my-app@1.0.0` stay in the core.
fn split_prerelease(head: &str) -> (&str, Option<&str>) {
    let bytes = head.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'-' && i > 0 && bytes[i - 1].is_ascii_digit() {
            return (&head[..i], Some(&head[i + 1..]));
        }
    }
    (head, None)
}

/// Zero-pad digit runs and lowercase everything else.
fn natural_key(section: &str) -> String {
    let mut key = String::with_capacity(section.len() + DIGIT_WIDTH);
    let mut rest = section;

    while !rest.is_empty() {
        let digits_len = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if digits_len > 0 {
            let digits = rest[..digits_len].trim_start_matches('0');
            // A run longer than the pad width can't be padded without losing
            // order against padded runs; length-prefixing keeps them comparable.
            if digits.len() > DIGIT_WIDTH {
                key.push('~');
                key.push_str(&format!("{:02}", digits.len().min(99)));
            }
            for _ in digits.len()..DIGIT_WIDTH {
                key.push('0');
            }
            key.push_str(digits);
            rest = &rest[digits_len..];
            continue;
        }

        let text_len = rest
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or(rest.len());
        key.extend(rest[..text_len].chars().flat_map(char::to_lowercase));
        rest = &rest[text_len..];
    }
    key
}

fn truncate_on_boundary(mut key: String) -> String {
    if key.len() > MAX_KEY_LEN {
        let cut = (0..=MAX_KEY_LEN)
            .rev()
            .find(|&i| key.is_char_boundary(i))
            .unwrap_or(0);
        key.truncate(cut);
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted_desc(versions: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = versions.iter().map(|s| s.to_string()).collect();
        v.sort_by(|a, b| version_sort_key(b).cmp(&version_sort_key(a)));
        v
    }

    #[test]
    fn numeric_segments_order_numerically() {
        assert_eq!(
            sorted_desc(&["1.0.9", "1.0.12", "1.0.10", "1.0.2"]),
            vec!["1.0.12", "1.0.10", "1.0.9", "1.0.2"]
        );
    }

    // The exact shape the OnesID project ships: package@semver+build.
    #[test]
    fn package_qualified_versions_order_by_version_within_package() {
        assert_eq!(
            sorted_desc(&[
                "com.softmax.did@1.0.4+111",
                "com.softmax.did@1.0.12+119",
                "com.softmax.did@1.0.9+116",
                "com.softmax.onesdid3@1.0.0+107",
            ]),
            vec![
                "com.softmax.onesdid3@1.0.0+107",
                "com.softmax.did@1.0.12+119",
                "com.softmax.did@1.0.9+116",
                "com.softmax.did@1.0.4+111",
            ]
        );
    }

    #[test]
    fn releases_outrank_their_own_prereleases() {
        assert_eq!(
            sorted_desc(&["2.0.0-rc.1", "2.0.0", "2.0.0-beta.1", "2.0.1"]),
            vec!["2.0.1", "2.0.0", "2.0.0-rc.1", "2.0.0-beta.1"]
        );
    }

    #[test]
    fn prerelease_numbers_order_numerically() {
        assert_eq!(
            sorted_desc(&["1.0.0-rc.2", "1.0.0-rc.10", "1.0.0-rc.1"]),
            vec!["1.0.0-rc.10", "1.0.0-rc.2", "1.0.0-rc.1"]
        );
    }

    // Only a hyphen after a digit starts a pre-release, so a hyphenated package
    // name keeps sorting on its version rather than splitting mid-name.
    #[test]
    fn hyphenated_package_names_are_not_prereleases() {
        assert_eq!(
            sorted_desc(&["my-app@1.0.2", "my-app@1.0.10"]),
            vec!["my-app@1.0.10", "my-app@1.0.2"]
        );
    }

    #[test]
    fn build_metadata_breaks_ties_without_outranking_the_core() {
        assert_eq!(
            sorted_desc(&["1.0.4+111", "1.0.4+99", "1.0.5+1"]),
            vec!["1.0.5+1", "1.0.4+111", "1.0.4+99"]
        );
    }

    #[test]
    fn leading_zeros_and_v_prefix_normalize() {
        assert_eq!(version_sort_key("v1.02"), version_sort_key("V1.2"));
    }

    #[test]
    fn oversized_digit_runs_stay_ordered() {
        let big = version_sort_key("1234567890123456");
        let small = version_sort_key("999999999999");
        assert!(big > small, "16-digit run must outrank a 12-digit one");
    }

    #[test]
    fn hash_style_releases_are_stable_and_distinct() {
        let a = version_sort_key("4646cc9bb4e8629a3f34c75b152f2abe1da1082a");
        assert_eq!(
            a,
            version_sort_key("4646cc9bb4e8629a3f34c75b152f2abe1da1082a")
        );
        assert_ne!(
            a,
            version_sort_key("aa46cc9bb4e8629a3f34c75b152f2abe1da1082a")
        );
    }

    #[test]
    fn empty_and_non_numeric_are_stable() {
        assert_eq!(version_sort_key(""), format!("{SEP}1{SEP}"));
        assert!(version_sort_key("nightly").starts_with("nightly"));
    }

    #[test]
    fn key_is_length_capped() {
        assert!(version_sort_key(&"a1".repeat(500)).len() <= MAX_KEY_LEN);
    }
}
