use askama::Template;
use axum::extract::{Path, Query, State};

use crate::extractors::{ProjectPath, ReadPool};
use crate::html::chrome::PageChrome;
use crate::html::utils::{render_project_detail, render_project_list, Chrome, ListParams};
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::{PagedResult, ReplaySummary};
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "replay_list.html")]
struct ReplayListTemplate {
    project_id: u64,
    result: PagedResult<ReplaySummary>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn list_handler(
    active: ActiveOrg,
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(axum::http::StatusCode::NOT_FOUND, "Not found".into()))?;
    let page = params.page.page();
    let result = queries::replays::list_replays(&pool, project_id, &page).await?;

    Ok(render_project_list(
        &pool,
        &state.nav_cache,
        project_id,
        chrome,
        result,
        |project_id, result, nav, chrome| ReplayListTemplate {
            project_id,
            result,
            nav,
            chrome,
        },
    )
    .await)
}

#[derive(Template)]
#[template(path = "replay_detail.html")]
struct ReplayDetailTemplate {
    project_id: u64,
    replay: queries::types::ReplayDetail,
    errors: Vec<queries::types::ReplayError>,
    raw_json: String,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

/// Recordings and videos are stored as opaque bytes, so the events API -- which
/// decodes payloads as JSON -- can't serve them in full.
fn replay_raw_json(replay: &queries::types::ReplayDetail) -> String {
    let full_payload_id =
        (replay.replay_type == "replay_event").then_some(replay.event_id.as_str());
    queries::event_supplements::render_raw_json(&replay.payload, full_payload_id)
}

pub async fn detail_handler(
    active: ActiveOrg,
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    Path((project_id, event_id)): Path<(u64, String)>,
) -> Result<axum::response::Response, HtmlError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(axum::http::StatusCode::NOT_FOUND, "Not found".into()))?;
    let replay = queries::replays::get_replay(&pool, project_id, &event_id).await?;
    let errors = match &replay {
        Some(r) => queries::replays::get_replay_errors(&pool, project_id, &r.payload).await?,
        None => Vec::new(),
    };

    render_project_detail(
        &pool,
        &state.nav_cache,
        project_id,
        chrome,
        replay,
        "Replay not found",
        move |project_id, replay, nav, chrome| {
            let raw_json = replay_raw_json(&replay);
            ReplayDetailTemplate {
                project_id,
                replay,
                errors,
                raw_json,
                nav,
                chrome,
            }
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use unic_langid::langid;

    const OVERSIZED: usize = 512 * 1024;

    fn detail(replay_type: &str, payload: serde_json::Value) -> queries::types::ReplayDetail {
        queries::types::ReplayDetail {
            event_id: "e1".into(),
            project_id: 1,
            timestamp: 0,
            replay_type: replay_type.into(),
            release: None,
            environment: None,
            payload,
        }
    }

    fn render(replay: queries::types::ReplayDetail) -> String {
        let raw_json = replay_raw_json(&replay);
        ReplayDetailTemplate {
            project_id: 1,
            replay,
            errors: Vec::new(),
            raw_json,
            nav: ProjectNavCounts::default(),
            chrome: PageChrome::new(String::new(), langid!("en"), "/web/projects/1/".into()),
        }
        .render()
        .expect("replay detail renders")
    }

    #[test]
    fn oversized_replay_event_is_truncated_with_the_api_hint() {
        let out = render(detail(
            "replay_event",
            serde_json::json!({"blob": "x".repeat(OVERSIZED)}),
        ));
        assert!(out.contains("[truncated: showing"), "{out:.400}");
        assert!(out.contains("/api/v1/events/e1/"));
        assert!(
            out.len() < OVERSIZED,
            "page must not carry the full payload"
        );
    }

    #[test]
    fn oversized_recording_is_truncated_without_an_api_hint() {
        let out = render(detail(
            "replay_recording",
            serde_json::Value::String("x".repeat(OVERSIZED)),
        ));
        assert!(out.contains("[truncated: showing"));
        assert!(
            !out.contains("/api/v1/events/"),
            "recordings are not served as JSON by the events API"
        );
        assert!(out.len() < OVERSIZED);
    }
}
