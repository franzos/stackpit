//! Login cookie semantics + synchronizer-token CSRF, against the running
//! server. Mirrors the e2e-review skill's tier-2 §2.3 probes.

use crate::common;

/// Every Set-Cookie the login response carries (session handle and CSRF salt).
async fn login_set_cookies() -> Vec<String> {
    let c = common::client();
    let resp = c
        .post(format!("{}/web/login", common::admin_url()))
        .form(&[("token", common::admin_token())])
        .send()
        .await
        .expect("POST /web/login");

    assert_eq!(resp.status().as_u16(), 303, "valid login should 303");
    assert_eq!(
        resp.headers().get("location").unwrap().to_str().unwrap(),
        "/web/projects/"
    );

    resp.headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect()
}

async fn login_session_cookie() -> String {
    login_set_cookies()
        .await
        .into_iter()
        .find(|c| c.starts_with("stackpit_token=") || c.starts_with("__Host-stackpit_token="))
        .expect("admin session Set-Cookie")
}

#[tokio::test]
async fn login_sets_random_session_cookie_and_redirects() {
    let set_cookie = login_session_cookie().await;

    assert!(set_cookie.contains("HttpOnly"), "cookie must be HttpOnly");
    assert!(
        set_cookie.contains("SameSite=Strict"),
        "cookie must be SameSite=Strict"
    );
    assert!(
        set_cookie.contains("Max-Age="),
        "cookie must carry an absolute expiry"
    );

    let value = |sc: &str| {
        sc.split_once('=')
            .unwrap()
            .1
            .split(';')
            .next()
            .unwrap()
            .to_string()
    };
    let first = value(&set_cookie);
    assert_eq!(first.len(), 64, "handle must be 64 hex chars");
    assert!(
        first.chars().all(|c| c.is_ascii_hexdigit()),
        "handle must be hex"
    );

    // Per-login random handle: a second login must mint a different value.
    let second = value(&login_session_cookie().await);
    assert_ne!(
        first, second,
        "cookie value must be random per login, not derived from admin_token"
    );
}

#[tokio::test]
async fn wrong_token_is_rejected() {
    let c = common::client();
    let resp = c
        .post(format!("{}/web/login", common::admin_url()))
        .form(&[("token", "definitely-wrong")])
        .send()
        .await
        .expect("POST /web/login");
    assert_eq!(resp.status().as_u16(), 401, "wrong token should 401");
}

#[tokio::test]
async fn logout_rejects_wrong_csrf_token() {
    let c = common::login().await;
    let logout_url = format!("{}/web/logout", common::admin_url());

    let wrong = c
        .post(&logout_url)
        .form(&[("csrf_token", "0".repeat(32).as_str())])
        .send()
        .await
        .expect("POST /web/logout");
    assert_eq!(wrong.status().as_u16(), 403, "wrong csrf -> 403");

    let csrf = common::csrf_token(&c, "/web/projects/").await;
    let ok = c
        .post(&logout_url)
        .form(&[("csrf_token", csrf.as_str())])
        .send()
        .await
        .expect("POST /web/logout");
    assert_eq!(ok.status().as_u16(), 303, "logout should 303");
    assert_eq!(
        ok.headers().get("location").unwrap().to_str().unwrap(),
        "/web/login"
    );
}

#[tokio::test]
async fn logout_revokes_admin_session_handle() {
    // Session handle plus CSRF salt: the admin CSRF token is derived from both.
    let cookie_header = login_set_cookies()
        .await
        .iter()
        .map(|sc| sc.split(';').next().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let session_pair = cookie_header
        .split("; ")
        .find(|p| p.starts_with("stackpit_token=") || p.starts_with("__Host-stackpit_token="))
        .unwrap()
        .to_string();

    // Jarless client so the replayed cookies are exactly what we set.
    let c = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let authed = c
        .get(format!("{}/web/projects/", common::admin_url()))
        .header("cookie", &cookie_header)
        .header("accept", "text/html")
        .send()
        .await
        .expect("GET projects with session cookie");
    assert_eq!(
        authed.status().as_u16(),
        200,
        "fresh handle must authenticate"
    );
    let csrf = common::extract_csrf(&authed.text().await.expect("projects body"))
        .expect("csrf_token hidden input on /web/projects/");

    let logout = c
        .post(format!("{}/web/logout", common::admin_url()))
        .header("cookie", &cookie_header)
        .form(&[("csrf_token", csrf.as_str())])
        .send()
        .await
        .expect("POST /web/logout");
    assert_eq!(logout.status().as_u16(), 303, "logout should 303");

    let replay = c
        .get(format!("{}/web/projects/", common::admin_url()))
        .header("cookie", &session_pair)
        .header("accept", "text/html")
        .send()
        .await
        .expect("GET projects with revoked cookie");
    assert_eq!(
        replay.status().as_u16(),
        303,
        "revoked handle must bounce to login, not authenticate"
    );
}

#[tokio::test]
async fn csrf_required_on_authenticated_post() {
    // Project 1 exists after seeding. Its settings page renders a csrf_token;
    // the rename endpoint enforces it.
    let c = common::login().await;
    let form_path = "/web/projects/1/settings/";
    let csrf = common::csrf_token(&c, form_path).await;
    let post_url = format!("{}/web/projects/1/settings/name", common::admin_url());

    // No token -> 403.
    let no_tok = c
        .post(&post_url)
        .form(&[("name", "csrf-probe")])
        .send()
        .await
        .expect("post no csrf");
    assert_eq!(no_tok.status().as_u16(), 403, "missing csrf -> 403");

    // Wrong token -> 403.
    let bad = "0".repeat(32);
    let wrong = c
        .post(&post_url)
        .form(&[("name", "csrf-probe"), ("csrf_token", bad.as_str())])
        .send()
        .await
        .expect("post wrong csrf");
    assert_eq!(wrong.status().as_u16(), 403, "wrong csrf -> 403");

    // Correct token -> success (303 redirect or 200).
    let ok = c
        .post(&post_url)
        .form(&[("name", "csrf-probe-ok"), ("csrf_token", csrf.as_str())])
        .send()
        .await
        .expect("post good csrf");
    assert!(
        matches!(ok.status().as_u16(), 200 | 303),
        "correct csrf should succeed, got {}",
        ok.status()
    );
}
