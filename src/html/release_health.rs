use askama::Template;
use axum::extract::{Path, State};

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::Csrf;
use crate::queries;
use crate::queries::types::ReleaseHealth;
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

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
    csrf_token: String,
}

/// Display row with crash-free metrics recomputed defensively so they can never
/// contradict each other, even on older/edge `session_aggregates` rows that
/// violate the crashed <= total invariant. `None` renders as "n/a".
struct ReleaseHealthRow {
    release: String,
    total_sessions: u64,
    ok_count: u64,
    crashed_count: u64,
    errored_count: u64,
    crash_free_sessions: Option<f64>,
    crash_free_users: Option<f64>,
}

impl From<ReleaseHealth> for ReleaseHealthRow {
    fn from(r: ReleaseHealth) -> Self {
        let crash_free_users = match (r.crash_free_users, r.total_users) {
            (Some(v), Some(users)) if users > 0 => Some(round2(v.clamp(0.0, 100.0))),
            _ => None,
        };
        Self {
            crash_free_sessions: crash_free_pct(r.total_sessions, r.crashed_count),
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

pub async fn handler(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
) -> Result<axum::response::Response, HtmlError> {
    let releases: Vec<ReleaseHealthRow> = queries::releases::get_release_health(&pool, project_id)
        .await?
        .into_iter()
        .map(ReleaseHealthRow::from)
        .collect();

    let since_ts = ((chrono::Utc::now().timestamp() - 86400 * 30) / 86400) * 86400;
    let daily = queries::releases::get_release_health_daily(&pool, project_id, since_ts)
        .await
        .unwrap_or_default();
    let chart = charts::render_session_chart(&daily).unwrap_or_default();

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = ReleaseHealthTemplate {
        project_id,
        releases,
        chart,
        nav,
        csrf_token: csrf,
    };
    Ok(render_template(&tmpl))
}
