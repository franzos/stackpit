/// The windows every list page offers, mirroring `m::period_options`. The empty
/// string is the "all time" option and is deliberately not in here.
pub const PERIODS: [&str; 7] = ["1h", "24h", "7d", "14d", "30d", "90d", "365d"];

/// Window a list page falls back to when neither `?period=` nor the browser
/// default picks one.
pub const DEFAULT_PERIOD: &str = "7d";

/// Turns period strings like "1h", "24h", "7d" into a Unix timestamp cutoff.
pub fn period_to_timestamp(period: &str) -> Option<i64> {
    let now = chrono::Utc::now().timestamp();
    let seconds = match period {
        "1h" => 3600,
        "24h" => 86400,
        "7d" => 7 * 86400,
        "14d" => 14 * 86400,
        "30d" => 30 * 86400,
        "90d" => 90 * 86400,
        "365d" => 365 * 86400,
        _ => return None,
    };
    Some(now - seconds)
}
