use askama::Template;

use crate::extractors::ProjectPageCtx;
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::queries;
use crate::queries::types::ReleaseHealth;
use crate::queries::ProjectNavCounts;

use super::charts;
use super::HtmlError;

// askama needs these filters in scope for template derivation
#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "release_health.html")]
struct ReleaseHealthTemplate {
    project_id: u64,
    releases: Vec<ReleaseHealthRow>,
    chart: String,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

/// Display row with crash-free metrics recomputed defensively so they can never
/// contradict each other, even on older/edge `session_aggregates` rows that
/// violate the crashed <= total invariant. `None` renders as "n/a".
/// Shared with the per-release detail page (`release_detail.rs`).
pub(crate) struct ReleaseHealthRow {
    pub(crate) release: String,
    pub(crate) total_sessions: u64,
    pub(crate) ok_count: u64,
    pub(crate) crashed_count: u64,
    pub(crate) errored_count: u64,
    pub(crate) crash_free_sessions: Option<f64>,
    pub(crate) error_free_sessions: Option<f64>,
    pub(crate) crash_free_users: Option<f64>,
}

impl From<ReleaseHealth> for ReleaseHealthRow {
    fn from(r: ReleaseHealth) -> Self {
        let crash_free_users = match (r.crash_free_users, r.total_users) {
            (Some(v), Some(users)) if users > 0 => Some(round2(v.clamp(0.0, 100.0))),
            _ => None,
        };
        Self {
            crash_free_sessions: crash_free_pct(r.total_sessions, r.crashed_count),
            error_free_sessions: error_free_pct(r.total_sessions, r.ok_count),
            crash_free_users,
            release: r.release,
            total_sessions: r.total_sessions,
            ok_count: r.ok_count,
            crashed_count: r.crashed_count,
            errored_count: r.errored_count,
        }
    }
}

fn round2(pct: f64) -> f64 {
    (pct * 100.0).round() / 100.0
}

/// Crash-free session percentage = (total - crashed) / total, clamped to
/// [0, 100]. Returns `None` (rendered "n/a") when total is 0, and never returns
/// exactly 100 while crashes exist.
fn crash_free_pct(total: u64, crashed: u64) -> Option<f64> {
    if total == 0 {
        return None;
    }
    let crashed = crashed.min(total);
    let pct = round2((total - crashed) as f64 / total as f64 * 100.0);
    let pct = if crashed > 0 { pct.min(99.99) } else { pct };
    Some(pct.clamp(0.0, 100.0))
}

/// Error-free (ok) session percentage = ok / total, where `ok` already excludes
/// crashed, errored, and abnormal sessions. Returns `None` (rendered "n/a") when
/// total is 0, and never returns exactly 100 while any non-ok session exists.
fn error_free_pct(total: u64, ok: u64) -> Option<f64> {
    if total == 0 {
        return None;
    }
    let ok = ok.min(total);
    let pct = round2(ok as f64 / total as f64 * 100.0);
    let pct = if ok < total { pct.min(99.99) } else { pct };
    Some(pct.clamp(0.0, 100.0))
}

pub async fn handler(ctx: ProjectPageCtx) -> Result<axum::response::Response, HtmlError> {
    let releases: Vec<ReleaseHealthRow> =
        queries::releases::get_release_health(&ctx.pool, ctx.project_id)
            .await?
            .into_iter()
            .map(ReleaseHealthRow::from)
            .collect();

    let since_ts = ((chrono::Utc::now().timestamp() - 86400 * 30) / 86400) * 86400;
    let daily = queries::releases::get_release_health_daily(&ctx.pool, ctx.project_id, since_ts)
        .await
        .unwrap_or_default();
    let chart = charts::render_session_chart(&daily).unwrap_or_default();

    let tmpl = ReleaseHealthTemplate {
        project_id: ctx.project_id,
        releases,
        chart,
        nav: ctx.nav,
        chrome: ctx.chrome,
    };
    Ok(render_template(&tmpl))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_free_zero_total_is_na() {
        assert_eq!(error_free_pct(0, 0), None);
    }

    #[test]
    fn error_free_all_ok_is_full() {
        assert_eq!(error_free_pct(100, 100), Some(100.0));
    }

    #[test]
    fn error_free_with_non_ok_never_full() {
        // 100 sessions, 5 errored/crashed/abnormal -> ok=95 -> 95%.
        assert_eq!(error_free_pct(100, 95), Some(95.0));
        // One non-ok session rounds to 99.99, not 100.
        assert_eq!(error_free_pct(100_000, 99_999), Some(99.99));
    }

    #[test]
    fn error_free_clamps_ok_over_total() {
        assert_eq!(error_free_pct(10, 20), Some(100.0));
    }
}
