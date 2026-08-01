use askama::Template;
use axum::extract::{Path, Query, State};

use crate::extractors::{ProjectPath, ReadPool};
use crate::html::chrome::PageChrome;
use crate::html::utils::{render_project_detail, render_project_list, Chrome, ListParams};
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::{PagedResult, ProfileSummary};
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "profile_list.html")]
struct ProfileListTemplate {
    project_id: u64,
    result: PagedResult<ProfileSummary>,
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
    let result = queries::profiles::list_profiles(&pool, project_id, &page).await?;

    Ok(render_project_list(
        &pool,
        &state.nav_cache,
        project_id,
        chrome,
        result,
        |project_id, result, nav, chrome| ProfileListTemplate {
            project_id,
            result,
            nav,
            chrome,
        },
    )
    .await)
}

#[derive(Template)]
#[template(path = "profile_detail.html")]
struct ProfileDetailTemplate {
    project_id: u64,
    profile: queries::types::ProfileDetail,
    raw_json: String,
    nav: ProjectNavCounts,
    chrome: PageChrome,
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
    let profile = queries::profiles::get_profile(&pool, project_id, &event_id).await?;

    render_project_detail(
        &pool,
        &state.nav_cache,
        project_id,
        chrome,
        profile,
        "Profile not found",
        |project_id, profile, nav, chrome| {
            let raw_json = queries::event_supplements::render_raw_json(
                &profile.payload,
                Some(&profile.event_id),
            );
            ProfileDetailTemplate {
                project_id,
                profile,
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

    #[test]
    fn oversized_profile_payload_is_truncated_with_the_api_hint() {
        let profile = queries::types::ProfileDetail {
            event_id: "e1".into(),
            timestamp: 0,
            transaction_name: None,
            platform: None,
            release: None,
            environment: None,
            payload: serde_json::json!({"blob": "x".repeat(OVERSIZED)}),
        };
        let raw_json =
            queries::event_supplements::render_raw_json(&profile.payload, Some(&profile.event_id));
        let out = ProfileDetailTemplate {
            project_id: 1,
            profile,
            raw_json,
            nav: ProjectNavCounts::default(),
            chrome: PageChrome::new(String::new(), langid!("en"), "/web/projects/1/".into()),
        }
        .render()
        .expect("profile detail renders");

        assert!(out.contains("[truncated: showing"), "{out:.400}");
        assert!(out.contains("/api/v1/events/e1/"));
        assert!(
            out.len() < OVERSIZED,
            "page must not carry the full payload"
        );
    }
}
