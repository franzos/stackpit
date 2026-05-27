//! Login cookie semantics + synchronizer-token CSRF, against the running
//! server. Mirrors the e2e-review skill's tier-2 §2.3 probes.

use crate::common;

#[tokio::test]
async fn login_sets_sha256_cookie_and_redirects() {
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

    let set_cookie = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .find(|c| c.starts_with("stackpit_token=") || c.starts_with("__Host-stackpit_token="))
        .expect("admin session Set-Cookie");

    assert!(set_cookie.contains("HttpOnly"), "cookie must be HttpOnly");
    assert!(
        set_cookie.contains("SameSite=Strict"),
        "cookie must be SameSite=Strict"
    );

    let value = set_cookie
        .split_once('=')
        .unwrap()
        .1
        .split(';')
        .next()
        .unwrap();
    assert_eq!(
        value,
        stackpit_auth::hash_token_for_cookie(&common::admin_token()),
        "cookie value must equal SHA-256(admin_token)"
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
