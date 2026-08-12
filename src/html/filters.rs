use std::convert::Infallible;

/// Formats a unix timestamp as a human-readable UTC datetime.
#[askama::filter_fn]
pub fn format_ts(ts: &i64, _: &dyn askama::Values) -> askama::Result<String, Infallible> {
    Ok(chrono::DateTime::from_timestamp(*ts, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| ts.to_string()))
}

/// Truncates to `max` *chars*, appending `…` when anything was cut.
///
/// `str::len` counts bytes, so a byte-indexed slice splits multi-byte UTF-8 and
/// panics. These inputs are SDK-supplied and attacker-controlled.
fn truncate_str(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => format!("{}…", &s[..idx]),
        None => s.to_string(),
    }
}

/// Truncates long IDs to 12 chars for display.
#[askama::filter_fn]
pub fn truncate_id(s: &str, _: &dyn askama::Values) -> askama::Result<String, Infallible> {
    Ok(truncate_str(s, 12))
}

/// Core of the `truncate_middle` filter, factored out so it is unit-testable.
fn truncate_middle_str(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max || max < 3 {
        return s.to_string();
    }
    // One char goes to the ellipsis; the head keeps the extra when max is even.
    let tail = (max - 1) / 2;
    let head = max - 1 - tail;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}

/// Truncates from the middle, keeping both ends.
///
/// Issue messages that share a long prefix differ only at the very end — a run
/// of `Job Failed: … Token is not active done_in=136 ms` rows is distinguished
/// solely by the trailing duration. End-clipping (CSS or otherwise) throws away
/// exactly the part that tells them apart.
/// Takes `impl AsRef<str>` rather than `&str` so it can be chained after a
/// filter that yields an owned `String` (`{{ t|split_error_message|truncate_middle(96) }}`).
#[askama::filter_fn]
pub fn truncate_middle(
    s: impl AsRef<str>,
    _: &dyn askama::Values,
    max: usize,
) -> askama::Result<String, Infallible> {
    Ok(truncate_middle_str(s.as_ref(), max))
}

/// Returns the error type from a "Type: message" title (before the first ": ").
#[askama::filter_fn]
pub fn split_error_type(title: &str, _: &dyn askama::Values) -> askama::Result<String, Infallible> {
    Ok(title
        .split_once(": ")
        .map(|(t, _)| t.to_string())
        .unwrap_or_else(|| title.to_string()))
}

/// Returns the error message from a "Type: message" title (after the first ": ").
#[askama::filter_fn]
pub fn split_error_message(
    title: &str,
    _: &dyn askama::Values,
) -> askama::Result<String, Infallible> {
    Ok(title
        .split_once(": ")
        .map(|(_, m)| m.to_string())
        .unwrap_or_default())
}

/// True for a version segment that looks like a VCS hash: long and all hex.
/// Semver-ish versions (dots, hyphens, short) are left untouched.
fn is_hashlike(s: &str) -> bool {
    s.len() >= 16 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Core of the `short_release` filter, factored out so it is unit-testable
/// (the `#[filter_fn]` macro rewrites the wrapper into a non-callable struct).
fn short_release_str(s: &str) -> String {
    if let Some((name, ver)) = s.rsplit_once('@') {
        if is_hashlike(ver) {
            return format!("{name}@{}…", &ver[..12]);
        }
        return s.to_string();
    }
    if is_hashlike(s) {
        return format!("{}…", &s[..12]);
    }
    s.to_string()
}

/// Shortens hash-like release versions for display while leaving semver-style
/// versions intact. `name@<40-hex>` becomes `name@<12-hex>…`; a bare long hash
/// becomes `<12-hex>…`; everything else is returned unchanged. The full value
/// is expected to be kept in a `title=` attribute at the call site.
#[askama::filter_fn]
pub fn short_release(s: &str, _: &dyn askama::Values) -> askama::Result<String, Infallible> {
    Ok(short_release_str(s))
}

/// Truncates URLs to 40 chars to keep the layout intact.
#[askama::filter_fn]
pub fn truncate_url(url: &str, _: &dyn askama::Values) -> askama::Result<String, Infallible> {
    Ok(truncate_str(url, 40))
}

/// Three significant digits: two decimals below 10, one at or above.
/// The threshold is tested on the *rounded* value, so 9.999 renders `10.0`
/// rather than a four-digit `10.00`.
fn three_digits(v: f64, unit: &str) -> String {
    if (v * 100.0).round() < 1000.0 {
        format!("{v:.2}{unit}")
    } else {
        format!("{v:.1}{unit}")
    }
}

/// Core of the `format_duration` filter, factored out so it is unit-testable.
fn format_duration_str(ms: i64) -> String {
    // Adaptive unit: `484ms`, `1.59s`, `19.5s`, `1.24min`, `1.00hr`. Precision
    // shrinks as magnitude grows, which keeps dense tables readable and matches
    // what a Sentry migrant expects.
    let v = ms as f64;
    if ms < 1_000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        three_digits(v / 1_000.0, "s")
    } else if ms < 3_600_000 {
        three_digits(v / 60_000.0, "min")
    } else {
        three_digits(v / 3_600_000.0, "hr")
    }
}

/// Formats a millisecond duration with an adaptive unit.
#[askama::filter_fn]
pub fn format_duration(ms: &i64, _: &dyn askama::Values) -> askama::Result<String, Infallible> {
    Ok(format_duration_str(*ms))
}

/// Core of the `format_pct` filter, factored out so it is unit-testable.
fn format_pct_str(pct: f64, decimals: usize) -> String {
    format!("{pct:.decimals$}%")
}

/// Formats a percentage to an explicit number of decimals, including the sign.
///
/// The decimal count is per call site, not global: `release_health` deliberately
/// clamps crash-free/error-free to `99.99` so they never read 100% while crashes
/// exist, and rounding those to one decimal would undo the clamp. Call sites keep
/// their own Rust-side rounding and only pass the digit count they render at.
#[askama::filter_fn]
pub fn format_pct(
    pct: &f64,
    _: &dyn askama::Values,
    decimals: usize,
) -> askama::Result<String, Infallible> {
    Ok(format_pct_str(*pct, decimals))
}

/// Formats a byte count as B/KB/MB/GB.
#[askama::filter_fn]
pub fn filesizeformat(size: &usize, _: &dyn askama::Values) -> askama::Result<String, Infallible> {
    let s = *size as f64;
    Ok(if s < 1024.0 {
        format!("{s:.0} B")
    } else if s < 1024.0 * 1024.0 {
        format!("{:.1} KB", s / 1024.0)
    } else if s < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", s / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", s / (1024.0 * 1024.0 * 1024.0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn short(s: &str) -> String {
        short_release_str(s)
    }

    #[test]
    fn short_release_shortens_bare_and_suffixed_hashes() {
        assert_eq!(
            short("4646cc9bb4e8629a3f34c75b152f2abe1da1082a"),
            "4646cc9bb4e8…"
        );
        assert_eq!(
            short("web@4646cc9bb4e8629a3f34c75b152f2abe1da1082a"),
            "web@4646cc9bb4e8…"
        );
    }

    #[test]
    fn short_release_leaves_semver_untouched() {
        assert_eq!(short("2.0.0-rc.1"), "2.0.0-rc.1");
        assert_eq!(short("Formshive@2.0.0"), "Formshive@2.0.0");
        assert_eq!(short("v1.2.3"), "v1.2.3");
    }

    /// 11 ASCII chars then `é`: 13 bytes, 12 chars. The old byte-indexed
    /// `&s[..12]` cut inside the two-byte `é` and panicked.
    #[test]
    fn truncate_str_does_not_split_a_multibyte_char() {
        let s = "aaaaaaaaaaaé";
        assert_eq!(s.len(), 13);
        assert_eq!(s.chars().count(), 12);
        assert_eq!(truncate_str(s, 12), s);

        let longer = "aaaaaaaaaaaéb";
        assert_eq!(truncate_str(longer, 12), "aaaaaaaaaaaé…");
    }

    #[test]
    fn truncate_str_cuts_on_char_count_not_bytes() {
        // 20 chars, 40 bytes: truncated by char count, never mid-char.
        let wide = "üüüüüüüüüüüüüüüüüüüü";
        assert_eq!(truncate_str(wide, 12), "üüüüüüüüüüüü…");
        assert_eq!(truncate_str("short", 12), "short");
        assert_eq!(truncate_str("", 12), "");
    }

    // The live case from project 40: sixteen issues whose messages share a ~120
    // char prefix and differ only in a trailing `done_in=NNN ms`.
    #[test]
    fn truncate_middle_keeps_the_distinguishing_tail() {
        let a = "Failed to send push notification to https://api.push.ones-now.com/v1/a/push/send (status 401 Unauthorized): Token is not active done_in=136 ms";
        let b = a.replace("136", "245");

        let ta = truncate_middle_str(a, 96);
        let tb = truncate_middle_str(&b, 96);
        assert_eq!(ta.chars().count(), 96);
        assert_ne!(ta, tb, "rows differing only at the end must still differ");
        assert!(ta.ends_with("done_in=136 ms"));
        assert!(tb.ends_with("done_in=245 ms"));
        assert!(ta.starts_with("Failed to send push notification"));
    }

    #[test]
    fn truncate_middle_leaves_short_strings_and_respects_char_boundaries() {
        assert_eq!(truncate_middle_str("short", 96), "short");
        // Exactly at the cap: untouched.
        let exact: String = "a".repeat(96);
        assert_eq!(truncate_middle_str(&exact, 96), exact);
        // One over: cut, still 96 chars wide.
        let over: String = "a".repeat(97);
        assert_eq!(truncate_middle_str(&over, 96).chars().count(), 96);
        // Multi-byte input must not panic or split a char.
        let wide: String = "ü".repeat(200);
        let cut = truncate_middle_str(&wide, 96);
        assert_eq!(cut.chars().count(), 96);
        assert!(cut.contains('…'));
        // A degenerate cap is a no-op rather than a panic.
        assert_eq!(truncate_middle_str("abcdef", 2), "abcdef");
    }

    #[test]
    fn format_duration_switches_unit_and_sheds_precision() {
        // Every worked example from the spec.
        assert_eq!(format_duration_str(0), "0ms");
        assert_eq!(format_duration_str(87), "87ms");
        assert_eq!(format_duration_str(484), "484ms");
        assert_eq!(format_duration_str(1_590), "1.59s");
        assert_eq!(format_duration_str(19_485), "19.5s");
        assert_eq!(format_duration_str(74_400), "1.24min");
        assert_eq!(format_duration_str(94_800), "1.58min");
        assert_eq!(format_duration_str(3_600_000), "1.00hr");
    }

    #[test]
    fn format_duration_boundaries() {
        assert_eq!(format_duration_str(999), "999ms");
        assert_eq!(format_duration_str(1_000), "1.00s");
        assert_eq!(format_duration_str(9_999), "10.0s");
        assert_eq!(format_duration_str(10_000), "10.0s");
        assert_eq!(format_duration_str(59_999), "60.0s");
        assert_eq!(format_duration_str(60_000), "1.00min");
        assert_eq!(format_duration_str(3_599_999), "60.0min");
        assert_eq!(format_duration_str(36_000_000), "10.0hr");
        // A negative duration is nonsense but must not panic or change unit.
        assert_eq!(format_duration_str(-5), "-5ms");
    }

    #[test]
    fn format_pct_honours_the_per_call_site_decimal_count() {
        assert_eq!(format_pct_str(95.0, 1), "95.0%");
        assert_eq!(format_pct_str(95.0, 2), "95.00%");
        // The release-health clamp must survive: 99.99 stays 99.99 at 2 decimals,
        // and is only allowed to read 100.0 where the call site asks for 1.
        assert_eq!(format_pct_str(99.99, 2), "99.99%");
        assert_eq!(format_pct_str(0.0, 1), "0.0%");
        assert_eq!(format_pct_str(100.0, 2), "100.00%");
    }

    #[test]
    fn truncate_str_uses_one_ellipsis_glyph() {
        let id = "0123456789abcdef";
        assert_eq!(truncate_str(id, 12), "0123456789ab…");
        assert!(!truncate_str(id, 12).contains("..."));
    }
}
