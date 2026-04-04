use askama::Template;
use axum::extract::Path;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;

use crate::server::AppState;

pub mod alerts;
pub mod bulk;
pub mod charts;
pub mod event_detail;
pub mod event_list;
pub mod event_type_list;
#[allow(dead_code)]
pub mod filters;
pub mod integrations;
pub mod issue_detail;
pub mod issue_list;
pub mod login;
pub mod logs;
pub mod metrics;
pub mod monitors;
pub mod new_project;
pub mod profiles;
pub mod project_filters;
pub mod project_integrations;
pub mod project_list;
pub mod project_settings;
pub mod release_health;
pub mod release_list;
pub mod replays;
pub mod spans;
pub mod transaction_list;
pub mod utils;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(|| async { axum::response::Redirect::permanent("/web/projects/") }),
        )
        // -- project routes --
        .route("/web/projects/", get(project_list::handler))
        .route(
            "/web/projects/new",
            get(new_project::form).post(new_project::create),
        )
        .route("/web/projects/{project_id}/", get(issue_list::handler))
        .route(
            "/web/projects/{project_id}/issues/{fingerprint}/",
            get(issue_detail::handler),
        )
        .route(
            "/web/projects/{project_id}/issues/{fingerprint}/status",
            post(issue_detail::update_status),
        )
        .route(
            "/web/projects/{project_id}/issues/{fingerprint}/discard",
            post(issue_detail::toggle_discard),
        )
        .route(
            "/web/projects/{project_id}/events/{event_id}/",
            get(event_detail::handler),
        )
        .route(
            "/web/projects/{project_id}/events/{event_id}/attachments/{filename}",
            get(event_detail::download_attachment),
        )
        .route(
            "/web/projects/{project_id}/transactions/",
            get(transaction_list::handler),
        )
        .route("/web/projects/{project_id}/logs/", get(logs::list_handler))
        .route(
            "/web/projects/{project_id}/spans/",
            get(spans::list_handler),
        )
        .route(
            "/web/projects/{project_id}/traces/{trace_id}/",
            get(spans::trace_detail_handler),
        )
        .route(
            "/web/projects/{project_id}/metrics/",
            get(metrics::list_handler),
        )
        .route(
            "/web/projects/{project_id}/metrics/{*mri}",
            get(metrics::detail_handler),
        )
        .route(
            "/web/projects/{project_id}/profiles/",
            get(profiles::list_handler),
        )
        .route(
            "/web/projects/{project_id}/profiles/{event_id}/",
            get(profiles::detail_handler),
        )
        .route(
            "/web/projects/{project_id}/replays/",
            get(replays::list_handler),
        )
        .route(
            "/web/projects/{project_id}/replays/{event_id}/",
            get(replays::detail_handler),
        )
        .route(
            "/web/projects/{project_id}/user-reports/",
            get(event_type_list::user_reports_handler),
        )
        .route(
            "/web/projects/{project_id}/client-reports/",
            get(event_type_list::client_reports_handler),
        )
        .route(
            "/web/projects/{project_id}/health/",
            get(release_health::handler),
        )
        .route(
            "/web/projects/{project_id}/monitors/",
            get(monitors::list_handler),
        )
        .route(
            "/web/projects/{project_id}/monitors/{slug}/",
            get(monitors::detail_handler),
        )
        .route(
            "/web/projects/{project_id}/settings/",
            get(project_settings::handler),
        )
        .route(
            "/web/projects/{project_id}/settings/name",
            post(project_settings::set_name),
        )
        .route(
            "/web/projects/{project_id}/settings/repos",
            post(project_settings::add_repo),
        )
        .route(
            "/web/projects/{project_id}/settings/repos/{repo_id}/delete",
            post(project_settings::delete_repo),
        )
        .route(
            "/web/projects/{project_id}/settings/keys/",
            get(project_settings::keys_handler),
        )
        .route(
            "/web/projects/{project_id}/settings/keys/create",
            post(project_settings::create_key),
        )
        .route(
            "/web/projects/{project_id}/settings/keys/{public_key}/delete",
            post(project_settings::delete_key),
        )
        .route(
            "/web/projects/{project_id}/settings/sourcemaps/",
            get(project_settings::sourcemaps_handler),
        )
        .route(
            "/web/projects/{project_id}/settings/sourcemaps/generate",
            post(project_settings::generate_sourcemap_key),
        )
        .route(
            "/web/projects/{project_id}/settings/archive",
            post(project_settings::archive_project),
        )
        .route(
            "/web/projects/{project_id}/settings/unarchive",
            post(project_settings::unarchive_project),
        )
        .route(
            "/web/projects/{project_id}/settings/delete",
            post(project_settings::delete_project),
        )
        // -- filter settings --
        .route(
            "/web/projects/{project_id}/settings/filters/",
            get(project_filters::handler),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/inbound",
            post(project_filters::set_inbound_filters),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/messages",
            post(project_filters::add_message_filter),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/messages/{id}/delete",
            post(project_filters::delete_message_filter),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/rate-limit",
            post(project_filters::set_rate_limit),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/environments",
            post(project_filters::add_environment_filter),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/environments/{id}/delete",
            post(project_filters::delete_environment_filter),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/releases",
            post(project_filters::add_release_filter),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/releases/{id}/delete",
            post(project_filters::delete_release_filter),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/user-agents",
            post(project_filters::add_ua_filter),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/user-agents/{id}/delete",
            post(project_filters::delete_ua_filter),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/rules",
            post(project_filters::add_filter_rule),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/rules/{id}/delete",
            post(project_filters::delete_filter_rule),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/ip-blocks",
            post(project_filters::add_ip_block),
        )
        .route(
            "/web/projects/{project_id}/settings/filters/ip-blocks/{id}/delete",
            post(project_filters::delete_ip_block),
        )
        // -- bulk operations --
        .route("/web/events/bulk", post(bulk::events_bulk))
        .route("/web/projects/{project_id}/bulk", post(bulk::issues_bulk))
        .route(
            "/web/projects/{project_id}/transactions/bulk",
            post(bulk::transactions_bulk),
        )
        .route(
            "/web/projects/{project_id}/user-reports/bulk",
            post(bulk::user_reports_bulk),
        )
        .route(
            "/web/projects/{project_id}/client-reports/bulk",
            post(bulk::client_reports_bulk),
        )
        .route(
            "/web/projects/{project_id}/monitors/{slug}/bulk",
            post(bulk::monitor_checkins_bulk),
        )
        // -- global settings: integrations --
        .route("/web/settings/integrations/", get(integrations::handler))
        .route(
            "/web/settings/integrations/new/webhook",
            get(integrations::new_webhook),
        )
        .route(
            "/web/settings/integrations/new/slack",
            get(integrations::new_slack),
        )
        .route(
            "/web/settings/integrations/new/email",
            get(integrations::new_email),
        )
        .route(
            "/web/settings/integrations/create",
            post(integrations::create),
        )
        .route(
            "/web/settings/integrations/{id}/delete",
            post(integrations::delete),
        )
        .route(
            "/web/settings/integrations/{id}/test",
            post(integrations::test_integration),
        )
        // -- global settings: alerts & digests --
        .route("/web/settings/alerts/", get(alerts::handler))
        .route(
            "/web/settings/alerts/rules/create",
            post(alerts::create_alert_rule),
        )
        .route(
            "/web/settings/alerts/rules/{id}/delete",
            post(alerts::delete_alert_rule),
        )
        .route(
            "/web/settings/alerts/digests/create",
            post(alerts::create_digest_schedule),
        )
        .route(
            "/web/settings/alerts/digests/{id}/delete",
            post(alerts::delete_digest_schedule),
        )
        // -- per-project integrations --
        .route(
            "/web/projects/{project_id}/settings/integrations/",
            get(project_integrations::handler),
        )
        .route(
            "/web/projects/{project_id}/settings/integrations/activate",
            post(project_integrations::activate),
        )
        .route(
            "/web/projects/{project_id}/settings/integrations/{id}/update",
            post(project_integrations::update),
        )
        .route(
            "/web/projects/{project_id}/settings/integrations/{id}/delete",
            post(project_integrations::deactivate),
        )
        // -- global views --
        .route("/web/events/", get(event_list::handler))
        .route("/web/releases/", get(release_list::handler))
        .route("/web/_assets/style.css", get(serve_css))
        .route("/web/_assets/icon.svg", get(serve_icon))
        // -- login --
        .route(
            "/web/login",
            get(login::login_form).post(login::handle_login),
        )
        .route("/web/logout", post(login::handle_logout))
        // -- legacy redirects --
        .route(
            "/web/",
            get(|| async { axum::response::Redirect::permanent("/web/projects/") }),
        )
        .route("/web/{project_id}/", get(redirect_old_project))
}

async fn redirect_old_project(Path(project_id): Path<u64>) -> impl IntoResponse {
    axum::response::Redirect::permanent(&format!("/web/projects/{project_id}/"))
}

async fn serve_css() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_str!("../../templates/style.css"),
    )
}

async fn serve_icon() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_str!("../../assets/icon.svg"),
    )
}

/// Renders a minimal styled error page -- nothing fancy, just enough to not look broken.
pub fn html_error(status: axum::http::StatusCode, detail: &str) -> axum::response::Response {
    let escaped_detail = html_escape(detail);
    let body = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="light dark"><title>Error - Stackpit</title>
<link rel="stylesheet" href="/web/_assets/style.css"></head>
<body>
<header><nav><a href="/web/projects/" class="nav-logo"><img src="/web/_assets/icon.svg" alt="Stackpit" width="24" height="24"> Stackpit</a></nav></header>
<main><h1>Error {}</h1><p>{}</p></main>
<footer><small>Stackpit</small></footer>
</body></html>"#,
        status.as_u16(),
        escaped_detail,
    );
    (status, axum::response::Html(body)).into_response()
}

/// Quick-and-dirty HTML escaping for error messages. Nothing fancy, just the usual suspects.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Tries to render an askama template -- falls back to an error page if it blows up.
pub fn render_template(tmpl: &impl Template) -> axum::response::Response {
    match tmpl.render() {
        Ok(body) => axum::response::Html(body).into_response(),
        Err(e) => html_error(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        ),
    }
}
