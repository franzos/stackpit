//! Security headers, static-asset serving, and health probes. Mirrors the
//! e2e-review skill's tier-2 §2.3/§2.6 probes.

use crate::common;

#[tokio::test]
async fn web_pages_carry_strict_csp_and_no_store() {
    let c = common::login().await;
    let resp = c
        .get(format!("{}/web/projects/", common::admin_url()))
        .send()
        .await
        .expect("GET /web/projects/");

    let csp = resp
        .headers()
        .get("content-security-policy")
        .expect("CSP header present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        csp.contains("script-src 'self'"),
        "CSP must allow self scripts"
    );
    // The hardening commits dropped `'unsafe-inline'` from script-src (external
    // JS files); style-src deliberately keeps it pending an inline-style
    // extraction pass (src/middleware/mod.rs:32). Scope the check to scripts so
    // it still fails if scripts ever regain inline.
    let script_src = csp
        .split(';')
        .map(str::trim)
        .find(|d| d.starts_with("script-src"))
        .expect("script-src directive present");
    assert!(
        !script_src.contains("'unsafe-inline'"),
        "script-src must NOT allow unsafe-inline, got: {script_src}"
    );
    assert!(csp.contains("object-src 'none'"), "CSP object-src none");
    assert!(csp.contains("base-uri 'self'"), "CSP base-uri self");
    assert!(csp.contains("form-action 'self'"), "CSP form-action self");

    let cache = resp
        .headers()
        .get("cache-control")
        .expect("Cache-Control present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(cache.contains("no-store"), "web pages must be no-store");
    assert!(cache.contains("private"), "web pages must be private");
}

#[tokio::test]
async fn static_assets_serve_with_correct_types() {
    let c = common::client();

    let js = [
        "/web/_assets/bulk.js",
        "/web/_assets/confirm.js",
        "/web/_assets/stop-propagation.js",
    ];
    for path in js {
        let resp = c
            .get(format!("{}{}", common::admin_url(), path))
            .send()
            .await
            .expect("GET asset");
        assert_eq!(resp.status().as_u16(), 200, "{path} -> 200");
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            ct.contains("application/javascript"),
            "{path} content-type was {ct}"
        );
    }

    for path in ["/web/_assets/style.css", "/web/_assets/icon.svg"] {
        let status = c
            .get(format!("{}{}", common::admin_url(), path))
            .send()
            .await
            .expect("GET asset")
            .status()
            .as_u16();
        assert_eq!(status, 200, "{path} -> 200");
    }
}

#[tokio::test]
async fn health_probes_return_ok() {
    for base in [common::admin_url(), common::ingest_url()] {
        let body = reqwest::get(format!("{base}/health"))
            .await
            .expect("GET /health")
            .text()
            .await
            .expect("body");
        assert_eq!(body, "ok", "{base}/health");
    }
}
