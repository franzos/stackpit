//! JSON API bearer auth. The admin token is a SHA-256-hashed shared bearer
//! accepted on `/api/v1/*`. Mirrors the e2e-review skill's tier-2 §2.5 probes.

use crate::common;

async fn get_status(path: &str, bearer: Option<&str>) -> u16 {
    let c = common::client();
    let mut req = c
        .get(format!("{}{}", common::admin_url(), path))
        .header("accept", "application/json");
    if let Some(b) = bearer {
        req = req.header("authorization", format!("Bearer {b}"));
    }
    req.send().await.expect("GET api").status().as_u16()
}

#[tokio::test]
async fn api_requires_valid_bearer() {
    let tok = common::admin_token();

    // Unparameterized endpoints: 200 with the token.
    for path in [
        "/api/v1/projects/",
        "/api/v1/alerts/rules",
        "/api/v1/digests",
    ] {
        assert_eq!(get_status(path, Some(&tok)).await, 200, "{path} with token");
    }

    // No bearer / wrong bearer -> 401.
    assert_eq!(
        get_status("/api/v1/projects/", None).await,
        401,
        "no bearer"
    );
    assert_eq!(
        get_status("/api/v1/projects/", Some("wrong")).await,
        401,
        "wrong bearer"
    );
}

#[tokio::test]
async fn project_scoped_endpoints_resolve() {
    let tok = common::admin_token();
    // Project 1 exists after seeding.
    for path in ["/api/v1/projects/1/issues/", "/api/v1/projects/1/events/"] {
        assert_eq!(get_status(path, Some(&tok)).await, 200, "{path} with token");
    }
}
