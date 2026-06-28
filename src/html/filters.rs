use std::convert::Infallible;

/// Formats a unix timestamp as a human-readable UTC datetime.
#[askama::filter_fn]
pub fn format_ts(ts: &i64, _: &dyn askama::Values) -> askama::Result<String, Infallible> {
    Ok(chrono::DateTime::from_timestamp(*ts, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| ts.to_string()))
}

/// Formats a timestamp as relative time (e.g. "3h ago", "2d ago").
#[askama::filter_fn]
pub fn format_relative(ts: &i64, _: &dyn askama::Values) -> askama::Result<String, Infallible> {
    let now = chrono::Utc::now().timestamp();
    let delta = now - *ts;

    if delta < 0 {
        return Ok("just now".to_string());
    }

    let secs = delta as u64;
    Ok(if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 604800 {
        format!("{}d ago", secs / 86400)
    } else {
        // Older than a week: show the date.
        chrono::DateTime::from_timestamp(*ts, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| ts.to_string())
    })
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
