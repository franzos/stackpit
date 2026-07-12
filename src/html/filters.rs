use std::convert::Infallible;

/// Formats a unix timestamp as a human-readable UTC datetime.
#[askama::filter_fn]
pub fn format_ts(ts: &i64, _: &dyn askama::Values) -> askama::Result<String, Infallible> {
    Ok(chrono::DateTime::from_timestamp(*ts, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| ts.to_string()))
}

/// Truncates long IDs to 12 chars for display.
#[askama::filter_fn]
pub fn truncate_id(s: &str, _: &dyn askama::Values) -> askama::Result<String, Infallible> {
    if s.len() > 12 {
        Ok(format!("{}...", &s[..12]))
    } else {
        Ok(s.to_string())
    }
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
    if url.len() <= 40 {
        Ok(url.to_string())
    } else {
        Ok(format!("{}...", &url[..37]))
    }
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
}
