//! Shared integration-test helpers: server URLs, admin-token discovery, an
//! authenticated reqwest client, CSRF extraction, and read-only DB queries.

#![allow(dead_code)]

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Admin (web + JSON API) base URL. Override with `STACKPIT_ADMIN_URL`.
pub fn admin_url() -> String {
    std::env::var("STACKPIT_ADMIN_URL").unwrap_or_else(|_| "http://127.0.0.1:3333".into())
}

/// Ingest (Sentry-protocol) base URL. Override with `STACKPIT_INGEST_URL`.
pub fn ingest_url() -> String {
    std::env::var("STACKPIT_INGEST_URL").unwrap_or_else(|_| "http://127.0.0.1:3334".into())
}

/// Admin token from `STACKPIT_ADMIN_TOKEN`, else discovered from `stackpit.toml`
/// at the repo root (same discovery the fake-data script uses).
pub fn admin_token() -> String {
    if let Ok(t) = std::env::var("STACKPIT_ADMIN_TOKEN") {
        return t;
    }
    discover_token_from_toml()
        .expect("admin_token: set STACKPIT_ADMIN_TOKEN or provide stackpit.toml at repo root")
}

fn discover_token_from_toml() -> Option<String> {
    let content = std::fs::read_to_string("stackpit.toml").ok()?;
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("admin_token") {
            let start = rest.find('"')? + 1;
            let rest = &rest[start..];
            let end = rest.find('"')?;
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// A non-redirect-following client with a cookie jar. Login/CSRF flows need
/// the jar; `redirect(none)` lets tests assert 303s instead of chasing them.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("build reqwest client")
}

/// Log in with the admin token; returns a client whose jar carries the session
/// cookie. Asserts the 303 so a misconfigured server fails loudly here.
pub async fn login() -> reqwest::Client {
    let c = client();
    let resp = c
        .post(format!("{}/web/login", admin_url()))
        .form(&[("token", admin_token())])
        .send()
        .await
        .expect("POST /web/login");
    assert_eq!(resp.status().as_u16(), 303, "login should redirect (303)");
    c
}

/// Fetch an authenticated form page and pull the synchronizer CSRF token out
/// of its hidden input.
pub async fn csrf_token(c: &reqwest::Client, form_path: &str) -> String {
    let body = c
        .get(format!("{}{}", admin_url(), form_path))
        .send()
        .await
        .expect("GET form page")
        .text()
        .await
        .expect("form body");
    extract_csrf(&body).unwrap_or_else(|| panic!("no csrf_token hidden input on {form_path}"))
}

fn extract_csrf(html: &str) -> Option<String> {
    let needle = "name=\"csrf_token\" value=\"";
    let i = html.find(needle)? + needle.len();
    let rest = &html[i..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Read-only query against `stackpit.db` via the sqlite3 CLI. Returns trimmed
/// stdout (tab/newline separated for multi-column/row results).
pub fn db_query(sql: &str) -> String {
    let out = Command::new("sqlite3")
        .arg("stackpit.db")
        .arg(sql)
        .output()
        .expect("run sqlite3 (is it on PATH?)");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Per-run project id well above the seeded range (1..=100), unique enough to
/// avoid collisions across repeated runs within the same wiped DB.
pub fn unique_project_id() -> u64 {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    900_000 + (ms % 90_000)
}

/// Execute a write statement against `stackpit.db` via the sqlite3 CLI.
pub fn db_exec(sql: &str) {
    Command::new("sqlite3")
        .arg("stackpit.db")
        .arg(sql)
        .output()
        .expect("run sqlite3 (is it on PATH?)");
}

/// Insert a native org directly into the DB and return its org_id (_c unused).
pub async fn seed_native_org(_c: &reqwest::Client, slug: &str) -> i64 {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    db_exec(&format!(
        "INSERT INTO organizations (slug, name, is_personal, created_at) \
         VALUES ('{slug}', '{slug}', 0, {ts})"
    ));
    db_query(&format!(
        "SELECT org_id FROM organizations WHERE slug = '{slug}' \
         ORDER BY org_id DESC LIMIT 1"
    ))
    .parse::<i64>()
    .expect("org_id parse")
}

/// Return true if an org row with the given id still exists.
pub async fn org_exists(org_id: i64) -> bool {
    db_query(&format!(
        "SELECT COUNT(*) FROM organizations WHERE org_id = {org_id}"
    )) == "1"
}
