//! Client text bound into columns: Postgres rejects `\0` and index rows over ~2.7 KB, failing the whole batch.

/// Cap on a stored text column, matching Sentry's limit for these fields.
pub(crate) const MAX_TEXT_CHARS: usize = 200;

/// Strip `\0` and truncate to [`MAX_TEXT_CHARS`] on a char boundary.
pub(crate) fn clean_text(s: &str) -> String {
    clean_text_max(s, MAX_TEXT_CHARS)
}

/// Strip `\0` without a length cap, for display-only text that isn't indexed.
pub(crate) fn strip_nul(s: &str) -> String {
    clean_text_max(s, usize::MAX)
}

/// Strip `\0` and truncate to `max` chars; a clean value that fits is copied unchanged.
pub(crate) fn clean_text_max(s: &str, max: usize) -> String {
    let fits = s.len() <= max || s.chars().nth(max).is_none();
    if fits && !s.contains('\0') {
        return s.to_owned();
    }
    s.chars().filter(|&c| c != '\0').take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_short_value_is_unchanged() {
        assert_eq!(clean_text("app@1.2.3"), "app@1.2.3");
        assert_eq!(clean_text("prod é ✓"), "prod é ✓");
    }

    #[test]
    fn strips_nul_anywhere() {
        assert_eq!(clean_text("\0a\0b\0"), "ab");
        assert_eq!(clean_text("\0"), "");
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(clean_text(""), "");
        assert_eq!(clean_text_max("", 0), "");
    }

    #[test]
    fn exactly_max_is_kept_and_one_more_is_cut() {
        let exact = "a".repeat(MAX_TEXT_CHARS);
        assert_eq!(clean_text(&exact), exact);
        let over = "a".repeat(MAX_TEXT_CHARS + 1);
        assert_eq!(clean_text(&over), exact);
    }

    #[test]
    fn truncates_multibyte_on_a_char_boundary() {
        let s = "é".repeat(MAX_TEXT_CHARS + 50);
        let out = clean_text(&s);
        assert_eq!(out.chars().count(), MAX_TEXT_CHARS);
        assert_eq!(out.len(), MAX_TEXT_CHARS * 2);

        // 200 two-byte chars are 400 bytes but still fit.
        let fits = "é".repeat(MAX_TEXT_CHARS);
        assert_eq!(clean_text(&fits), fits);
    }

    #[test]
    fn cap_counts_chars_after_stripping_nul() {
        let s = format!("\0{}", "a".repeat(MAX_TEXT_CHARS));
        assert_eq!(clean_text(&s), "a".repeat(MAX_TEXT_CHARS));
    }

    #[test]
    fn unbounded_variant_only_strips_nul() {
        let long = format!("{}\0x", "a".repeat(5000));
        let out = clean_text_max(&long, usize::MAX);
        assert_eq!(out.len(), 5001);
        assert!(!out.contains('\0'));
    }
}
