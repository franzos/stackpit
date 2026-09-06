use askama::Template;
use axum::extract::{Form, Path, State};
use axum::response::{IntoResponse, Redirect};
use serde::Deserialize;

use crate::html::chrome::PageChrome;
use crate::html::utils::Chrome;
use crate::html::{render_template, HtmlError};
use crate::orgs::extractor::{require_superuser, ActiveOrg};
use crate::queries::orgs::{list_non_system_orgs, OrgSummary};
use crate::queries::projects::{list_unassigned_projects, reassign_project, UnassignedProject};
use crate::server::AppState;

#[derive(Template)]
#[template(path = "admin_unassigned.html")]
struct UnassignedTemplate {
    projects: Vec<UnassignedProject>,
    orgs: Vec<OrgSummary>,
    chrome: PageChrome,
}

/// `GET /web/admin/unassigned`: superuser-only; lists projects still in the system org.
pub async fn unassigned_view(
    State(state): State<AppState>,
    active: ActiveOrg,
    Chrome(chrome): Chrome,
) -> Result<axum::response::Response, HtmlError> {
    if let Err(r) = require_superuser(&active) {
        return Ok(r);
    }
    let projects = list_unassigned_projects(&state.pool).await?;
    let orgs = list_non_system_orgs(&state.pool).await?;
    Ok(render_template(&UnassignedTemplate {
        projects,
        orgs,
        chrome,
    }))
}

#[derive(Deserialize)]
pub struct AssignForm {
    org_id: i64,
}

/// `POST /web/admin/projects/{id}/assign`: superuser-only; moves a project to the given org.
pub async fn assign_project(
    State(state): State<AppState>,
    active: ActiveOrg,
    Path(project_id): Path<i64>,
    Form(form): Form<AssignForm>,
) -> Result<axum::response::Response, HtmlError> {
    if let Err(r) = require_superuser(&active) {
        return Ok(r);
    }
    reassign_project(&state.writer_pool, project_id, form.org_id).await?;
    Ok(Redirect::to("/web/admin/unassigned").into_response())
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    // Pre-i18n-retrofit baseline: empty lists render the deterministic empty state.
    #[test]
    fn admin_unassigned_renders_stable() {
        let tmpl = UnassignedTemplate {
            projects: Vec::new(),
            orgs: Vec::new(),
            chrome: PageChrome::new(
                "test-csrf-token".into(),
                crate::locale::default_locale(),
                "/web/projects/".into(),
            ),
        };
        insta::assert_snapshot!(tmpl.render().unwrap());
    }

    // Populated render in en and de exercises the table-branch keys (columns,
    // id prefix, assign) that the empty-state snapshot does not reach.
    #[test]
    fn admin_unassigned_renders_populated_without_missing_keys() {
        use unic_langid::langid;
        for locale in [langid!("en"), langid!("de")] {
            let tmpl = UnassignedTemplate {
                projects: vec![UnassignedProject {
                    project_id: 7,
                    name: Some("Demo".into()),
                    source: Some("sdk".into()),
                }],
                orgs: vec![OrgSummary {
                    org_id: 2,
                    slug: "acme".into(),
                    name: Some("Acme".into()),
                }],
                chrome: PageChrome::new("csrf".into(), locale.clone(), "/web/projects/".into()),
            };
            let html = tmpl.render().expect("admin unassigned renders");
            assert!(
                !html.contains(crate::i18n::MISSING_PREFIX),
                "admin unassigned ({locale}) leaked a missing localization key: {html}"
            );
        }
    }
}
