use askama::Template;
use axum::extract::Path;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;

use crate::server::AppState;

pub mod alerts;
pub mod auth;
pub mod browser_defaults;
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
pub mod transactions;
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
            get(transactions::list_handler),
        )
        .route(
            "/web/projects/{project_id}/transactions/detail",
            get(transactions::detail_handler),
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
        // -- global settings: browser defaults --
        .route(
            "/web/settings/defaults/",
            get(browser_defaults::handler).post(browser_defaults::save_defaults),
        )
        .route(
            "/web/settings/defaults/clear",
            post(browser_defaults::clear_defaults),
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
        .route("/web/_assets/bulk.js", get(serve_bulk_js))
        .route("/web/_assets/confirm.js", get(serve_confirm_js))
        .route(
            "/web/_assets/stop-propagation.js",
            get(serve_stop_propagation_js),
        )
        .route(
            "/web/_assets/fonts/Inter-Regular.woff2",
            get(serve_font_inter_regular),
        )
        .route(
            "/web/_assets/fonts/Inter-Medium.woff2",
            get(serve_font_inter_medium),
        )
        .route(
            "/web/_assets/fonts/Inter-SemiBold.woff2",
            get(serve_font_inter_semibold),
        )
        .route(
            "/web/_assets/fonts/Inter-Bold.woff2",
            get(serve_font_inter_bold),
        )
        .route(
            "/web/_assets/fonts/JetBrainsMono-Regular.woff2",
            get(serve_font_jbm_regular),
        )
        .route(
            "/web/_assets/fonts/JetBrainsMono-Medium.woff2",
            get(serve_font_jbm_medium),
        )
        // -- login --
        .route(
            "/web/login",
            get(login::login_form).post(login::handle_login),
        )
        .route("/web/logout", post(login::handle_logout))
        // -- SSO (OAuth/OIDC) --
        .route("/web/auth/login", get(auth::login))
        .route("/web/auth/callback", get(auth::callback))
        .route(
            "/web/auth/backchannel-logout",
            post(auth::backchannel_logout),
        )
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

async fn serve_bulk_js() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_str!("../../static/bulk.js"),
    )
}

async fn serve_confirm_js() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_str!("../../static/confirm.js"),
    )
}

async fn serve_stop_propagation_js() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_str!("../../static/stop-propagation.js"),
    )
}

fn font_response(bytes: &'static [u8]) -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "font/woff2"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    )
}

async fn serve_font_inter_regular() -> impl IntoResponse {
    font_response(include_bytes!("../../assets/fonts/Inter-Regular.woff2"))
}
async fn serve_font_inter_medium() -> impl IntoResponse {
    font_response(include_bytes!("../../assets/fonts/Inter-Medium.woff2"))
}
async fn serve_font_inter_semibold() -> impl IntoResponse {
    font_response(include_bytes!("../../assets/fonts/Inter-SemiBold.woff2"))
}
async fn serve_font_inter_bold() -> impl IntoResponse {
    font_response(include_bytes!("../../assets/fonts/Inter-Bold.woff2"))
}
async fn serve_font_jbm_regular() -> impl IntoResponse {
    font_response(include_bytes!(
        "../../assets/fonts/JetBrainsMono-Regular.woff2"
    ))
}
async fn serve_font_jbm_medium() -> impl IntoResponse {
    font_response(include_bytes!(
        "../../assets/fonts/JetBrainsMono-Medium.woff2"
    ))
}

/// Error type for HTML handlers. Renders through `html_error` so the page
/// looks identical. `From<anyhow::Error>` maps to 500 so query calls can use `?`.
pub struct HtmlError(pub axum::http::StatusCode, pub String);

impl IntoResponse for HtmlError {
    fn into_response(self) -> axum::response::Response {
        html_error(self.0, &self.1)
    }
}

impl From<anyhow::Error> for HtmlError {
    fn from(e: anyhow::Error) -> Self {
        HtmlError(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}

/// Minimal styled error page. Uses the same shell language as base.html but
/// without a sidebar so the page renders standalone.
pub fn html_error(status: axum::http::StatusCode, detail: &str) -> axum::response::Response {
    let escaped_detail = crate::encoding::escape_html(detail);
    let body = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="light dark"><title>Error - Stackpit</title>
<link rel="preload" href="/web/_assets/fonts/Inter-Regular.woff2" as="font" type="font/woff2" crossorigin>
<link rel="preload" href="/web/_assets/fonts/Inter-SemiBold.woff2" as="font" type="font/woff2" crossorigin>
<link rel="stylesheet" href="/web/_assets/style.css">
<link rel="icon" type="image/svg+xml" href="/web/_assets/icon.svg"></head>
<body>
<div class="min-h-screen flex items-center justify-center px-6">
<div class="card card-pad max-w-lg w-full">
<div class="flex items-center gap-2 mb-4"><img src="/web/_assets/icon.svg" alt="" width="22" height="22"><span class="font-semibold">Stackpit</span></div>
<div class="page-h1 mb-2">Error {}</div>
<p class="text-muted">{}</p>
<div class="mt-6"><a href="/web/projects/" class="btn btn-secondary">Back to projects</a></div>
</div>
</div>
</body></html>"#,
        status.as_u16(),
        escaped_detail,
    );
    (status, axum::response::Html(body)).into_response()
}

/// Renders an askama template; falls back to an error page on failure.
pub fn render_template(tmpl: &impl Template) -> axum::response::Response {
    match tmpl.render() {
        Ok(body) => axum::response::Html(body).into_response(),
        Err(e) => html_error(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        ),
    }
}
