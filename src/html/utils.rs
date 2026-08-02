use std::collections::HashMap;

use askama::Template;
use axum::extract::FromRef;
use axum::http::StatusCode;
use serde::Deserialize;

use crate::server::AppState;

use crate::db::DbPool;
use crate::html::chrome::PageChrome;
use crate::html::{render_template, HtmlError};
use crate::middleware::CsrfToken;
use crate::queries;
use crate::queries::projects::NavCountsCache;
use crate::queries::types::Pagination;
use crate::queries::ProjectNavCounts;

/// The persisted `preferred_language` for the current user, stashed by the
/// web-auth middleware on the sp_grant path. Absent for admin-token and
/// no-auth modes (no user row), so readers treat it as `Option`.
#[derive(Clone)]
pub struct PreferredLanguage(pub Option<String>);

/// Builds the per-page shell (CSRF + resolved locale) from request extensions.
/// Infallible, mirroring [`Csrf`]: reads the CSRF token the same way and
/// resolves the locale via the ladder, defaulting to `en` when nothing matches.
pub struct Chrome(pub PageChrome);

impl<S> axum::extract::FromRequestParts<S> for Chrome
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let csrf = parts
            .extensions
            .get::<CsrfToken>()
            .map(|t| t.0.clone())
            .unwrap_or_default();
        let pref = parts
            .extensions
            .get::<PreferredLanguage>()
            .and_then(|p| p.0.clone());
        let locale = crate::html::chrome::resolve_locale(parts, pref.as_deref());
        let path = parts
            .uri
            .path_and_query()
            .map(|p| p.as_str())
            .unwrap_or("/web/projects/")
            .to_string();
        let active_org = parts
            .extensions
            .get::<crate::orgs::extractor::ActiveOrg>()
            .and_then(|a| a.org_name.clone());
        let state = AppState::from_ref(state);
        let status = state.license.status();
        Ok(Chrome(
            PageChrome::new(csrf, locale, path)
                .with_license_watermark(&status)
                .with_active_org(active_org),
        ))
    }
}

/// Runs an admin DB write and renders a success/error response. Success
/// renders the message and drops the value, so handlers that ignore the
/// returned id can still use this.
pub async fn query_then_render<T, F, Fut>(
    result: anyhow::Result<T>,
    chrome: &PageChrome,
    success_msg: &str,
    render: F,
) -> axum::response::Response
where
    F: FnOnce(Option<String>) -> Fut,
    Fut: std::future::Future<Output = axum::response::Response>,
{
    match result {
        Ok(_) => render(Some(success_msg.to_string())).await,
        Err(e) => render(Some(chrome.err(e))).await,
    }
}

/// Fetches the project nav counts and renders a per-project list page.
/// `build` supplies the page-specific template; this owns the shared
/// nav-counts and render boilerplate.
pub async fn render_project_list<T, F, Tmpl>(
    pool: &DbPool,
    cache: &NavCountsCache,
    project_id: u64,
    chrome: PageChrome,
    result: T,
    build: F,
) -> axum::response::Response
where
    F: FnOnce(u64, T, ProjectNavCounts, PageChrome) -> Tmpl,
    Tmpl: Template,
{
    let nav = queries::projects::nav_counts_cached(pool, cache, project_id).await;
    render_template(&build(project_id, result, nav, chrome))
}

/// Like [`render_project_list`] for detail pages: resolves an `Option` row,
/// 404s with `not_found` when absent, then renders via `build`.
pub async fn render_project_detail<T, F, Tmpl>(
    pool: &DbPool,
    cache: &NavCountsCache,
    project_id: u64,
    chrome: PageChrome,
    item: Option<T>,
    not_found: &str,
    build: F,
) -> Result<axum::response::Response, HtmlError>
where
    F: FnOnce(u64, T, ProjectNavCounts, PageChrome) -> Tmpl,
    Tmpl: Template,
{
    let Some(item) = item else {
        return Err(HtmlError(StatusCode::NOT_FOUND, not_found.into()));
    };
    let nav = queries::projects::nav_counts_cached(pool, cache, project_id).await;
    Ok(render_template(&build(project_id, item, nav, chrome)))
}

/// Shared query params for all list pages. Unused fields stay `None`.
#[derive(Deserialize)]
pub struct ListParams {
    pub query: Option<String>,
    pub level: Option<String>,
    pub status: Option<String>,
    pub sort: Option<String>,
    pub release: Option<String>,
    pub tag: Option<String>,
    pub period: Option<String>,
    /// Org filter for the cross-org project list.
    pub org: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub project_id: Option<u64>,
    pub item_type: Option<String>,
    #[serde(flatten)]
    pub page: Pagination,
}

/// Empty-string-to-`None` filtering shared by the list-page filter builders.
fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.is_empty())
}

/// Builds an [`EventFilter`] from the shared list params, treating blank
/// fields as absent.
pub fn event_filter_from_params(params: &ListParams) -> queries::types::EventFilter {
    queries::types::EventFilter {
        level: non_empty(params.level.clone()),
        project_id: params.project_id,
        query: non_empty(params.query.clone()),
        sort: non_empty(params.sort.clone()),
        item_type: non_empty(params.item_type.clone()),
    }
}

/// Builds an [`IssueFilter`] from the shared list params for the given item
/// type. `tag` is parsed `key=value` (both sides non-empty) or dropped.
pub fn issue_filter_from_params(
    params: &ListParams,
    item_type: &str,
) -> queries::types::IssueFilter {
    let tag = params
        .tag
        .as_deref()
        .and_then(|t| t.split_once('='))
        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        .map(|(k, v)| (k.to_string(), v.to_string()));
    queries::types::IssueFilter {
        level: non_empty(params.level.clone()),
        status: non_empty(params.status.clone()),
        query: non_empty(params.query.clone()),
        sort: non_empty(params.sort.clone()),
        item_type: Some(item_type.to_string()),
        release: non_empty(params.release.clone()),
        tag,
    }
}

/// Treats empty or whitespace-only strings as `None` for optional numeric
/// form/query fields (browsers submit a blank input as `field=`).
pub fn empty_string_as_none<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(v) => v.parse().map(Some).map_err(serde::de::Error::custom),
    }
}

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

/// Builds the query strings for pagination and filtering. `sort` belongs
/// only in filter_qs, not in pagination links.
pub fn build_filter_qs(params: &[(&str, &str)], sort: &str) -> (String, String) {
    let mut base_parts = Vec::new();
    for (name, value) in params {
        if !value.is_empty() {
            base_parts.push(format!("&{}={}", name, urlencoded(value)));
        }
    }
    let base_qs = base_parts.join("");
    let mut filter_qs = base_qs.clone();
    if !sort.is_empty() {
        filter_qs.push_str(&format!("&sort={}", urlencoded(sort)));
    }
    (base_qs, filter_qs)
}

/// Percent-encodes a query-string value (`application/x-www-form-urlencoded`, space-as-`+`).
fn urlencoded(s: &str) -> String {
    form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

pub const DEFAULTS_COOKIE: &str = "sp_defaults";

/// Parses the `sp_defaults` cookie value (format: `status:resolved|period:7d`)
/// into a map. Invalid segments are silently skipped.
pub fn parse_defaults_cookie(value: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for segment in value.split('|') {
        if let Some((k, v)) = segment.split_once(':') {
            let k = k.trim();
            let v = v.trim();
            if !k.is_empty() && !v.is_empty() {
                map.insert(k.to_string(), v.to_string());
            }
        }
    }
    map
}

/// Serializes a defaults map back into cookie format: `status:resolved|period:7d`.
pub fn serialize_defaults_cookie(defaults: &HashMap<String, String>) -> String {
    let mut parts: Vec<String> = defaults
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k}:{v}"))
        .collect();
    parts.sort(); // deterministic order
    parts.join("|")
}

/// Checks which applicable keys are missing from the query string but present
/// in cookie defaults. Returns a redirect URL with those defaults appended,
/// preserving all existing params. Returns `None` when nothing needs adding.
pub fn defaults_redirect_url(
    path: &str,
    raw_qs: Option<&str>,
    defaults: &HashMap<String, String>,
    applicable_keys: &[&str],
) -> Option<String> {
    let qs = raw_qs.unwrap_or("");

    // Collect keys already present in the query string (even if empty-valued).
    let existing_keys: std::collections::HashSet<&str> = qs
        .split('&')
        .filter_map(|pair| pair.split_once('=').map(|(k, _)| k))
        .collect();

    let mut additions = Vec::new();
    for &key in applicable_keys {
        if !existing_keys.contains(key) {
            if let Some(val) = defaults.get(key) {
                if !val.is_empty() {
                    additions.push(format!("{key}={}", urlencoded(val)));
                }
            }
        }
    }
    if additions.is_empty() {
        return None;
    }

    let merged = if qs.is_empty() {
        additions.join("&")
    } else {
        format!("{qs}&{}", additions.join("&"))
    };
    Some(format!("{path}?{merged}"))
}
