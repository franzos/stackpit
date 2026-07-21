use crate::domain::IntegrationKind;
use serde::Deserialize;

/// Issue-creation responses are a few KB; cap the read so a malicious or
/// misbehaving tracker endpoint can't stream unbounded data into memory.
const MAX_TRACKER_RESP: usize = 256 * 1024;
/// Trackers cap issue titles; Stackpit issue titles are raw error strings.
const MAX_TITLE: usize = 240;

pub struct TrackerTarget {
    pub base_url: String,
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub project_id: Option<i64>,
}

pub struct NewExternalIssue<'a> {
    pub title: &'a str,
    pub body: &'a str,
}

#[derive(Debug)]
pub struct CreatedExternalIssue {
    pub external_id: String,
    pub external_url: String,
}

pub fn issue_api_url(kind: IntegrationKind, target: &TrackerTarget) -> anyhow::Result<String> {
    let base = target.base_url.trim_end_matches('/');
    match kind {
        IntegrationKind::GitHub | IntegrationKind::Forgejo => {
            let owner = target
                .owner
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing owner"))?;
            let repo = target
                .repo
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing repo"))?;
            Ok(format!("{base}/repos/{owner}/{repo}/issues"))
        }
        IntegrationKind::GitLab => {
            let project_id = target
                .project_id
                .ok_or_else(|| anyhow::anyhow!("missing project_id"))?;
            Ok(format!("{base}/api/v4/projects/{project_id}/issues"))
        }
        IntegrationKind::Webhook | IntegrationKind::Slack | IntegrationKind::Email => {
            anyhow::bail!("not a tracker kind")
        }
    }
}

fn capped_title(title: &str) -> String {
    title.chars().take(MAX_TITLE).collect()
}

pub async fn create_issue(
    client: &reqwest::Client,
    kind: IntegrationKind,
    target: &TrackerTarget,
    token: &str,
    issue: &NewExternalIssue<'_>,
) -> anyhow::Result<CreatedExternalIssue> {
    let url = issue_api_url(kind, target)?;
    let title = capped_title(issue.title);
    let req = match kind {
        IntegrationKind::GitHub | IntegrationKind::Forgejo => client
            .post(&url)
            .header("Authorization", format!("token {token}"))
            .header("User-Agent", "stackpit")
            .json(&serde_json::json!({ "title": title, "body": issue.body })),
        IntegrationKind::GitLab => client
            .post(&url)
            .header("PRIVATE-TOKEN", token)
            .json(&serde_json::json!({ "title": title, "description": issue.body })),
        IntegrationKind::Webhook | IntegrationKind::Slack | IntegrationKind::Email => {
            anyhow::bail!("not a tracker kind")
        }
    };

    // Own send/status-check here (not send_and_check, which discards the body
    // we need). Status only in errors, never the body -- a 4xx can reflect
    // submitted input, so keep it out of the error message.
    let resp = req.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("tracker.create_issue returned {}", resp.status());
    }
    let body = read_capped(resp, MAX_TRACKER_RESP).await?;
    parse_created(kind, &body)
}

async fn read_capped(mut resp: reqwest::Response, max: usize) -> anyhow::Result<String> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        buf.extend_from_slice(&chunk);
        if buf.len() > max {
            anyhow::bail!("tracker response exceeded {max} bytes");
        }
    }
    Ok(String::from_utf8(buf)?)
}

fn parse_created(kind: IntegrationKind, body: &str) -> anyhow::Result<CreatedExternalIssue> {
    let created = match kind {
        IntegrationKind::GitHub | IntegrationKind::Forgejo => {
            #[derive(Deserialize)]
            struct Resp {
                number: i64,
                html_url: String,
            }
            let r: Resp = serde_json::from_str(body)?;
            CreatedExternalIssue {
                external_id: r.number.to_string(),
                external_url: r.html_url,
            }
        }
        IntegrationKind::GitLab => {
            #[derive(Deserialize)]
            struct Resp {
                iid: i64,
                web_url: String,
            }
            let r: Resp = serde_json::from_str(body)?;
            CreatedExternalIssue {
                external_id: r.iid.to_string(),
                external_url: r.web_url,
            }
        }
        IntegrationKind::Webhook | IntegrationKind::Slack | IntegrationKind::Email => {
            anyhow::bail!("not a tracker kind")
        }
    };

    // external_url comes from the (possibly self-hosted/hostile) tracker
    // response and is later rendered as an href; reject non-http(s) schemes
    // to close off javascript:/data: XSS.
    let scheme_ok =
        created.external_url.starts_with("https://") || created.external_url.starts_with("http://");
    if !scheme_ok {
        anyhow::bail!("tracker returned a non-http(s) issue url");
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::IntegrationKind;

    #[test]
    fn issue_api_url_github_and_forgejo_use_repos_path() {
        let target = TrackerTarget {
            base_url: "https://github.example".into(),
            owner: Some("acme".into()),
            repo: Some("backend".into()),
            project_id: None,
        };
        assert_eq!(
            issue_api_url(IntegrationKind::GitHub, &target).unwrap(),
            "https://github.example/repos/acme/backend/issues"
        );
        assert_eq!(
            issue_api_url(IntegrationKind::Forgejo, &target).unwrap(),
            "https://github.example/repos/acme/backend/issues"
        );
    }

    #[test]
    fn issue_api_url_missing_owner_or_repo_errors() {
        let no_owner = TrackerTarget {
            base_url: "https://github.example".into(),
            owner: None,
            repo: Some("backend".into()),
            project_id: None,
        };
        assert!(issue_api_url(IntegrationKind::GitHub, &no_owner).is_err());

        let no_repo = TrackerTarget {
            base_url: "https://github.example".into(),
            owner: Some("acme".into()),
            repo: None,
            project_id: None,
        };
        assert!(issue_api_url(IntegrationKind::Forgejo, &no_repo).is_err());
    }

    #[test]
    fn issue_api_url_gitlab_uses_project_id_path() {
        let target = TrackerTarget {
            base_url: "https://gitlab.example/".into(),
            owner: None,
            repo: None,
            project_id: Some(123),
        };
        assert_eq!(
            issue_api_url(IntegrationKind::GitLab, &target).unwrap(),
            "https://gitlab.example/api/v4/projects/123/issues"
        );
    }

    #[test]
    fn issue_api_url_gitlab_missing_project_id_errors() {
        let target = TrackerTarget {
            base_url: "https://gitlab.example".into(),
            owner: None,
            repo: None,
            project_id: None,
        };
        assert!(issue_api_url(IntegrationKind::GitLab, &target).is_err());
    }

    #[test]
    fn parse_created_github_and_forgejo_shape() {
        let body = r#"{"number":42,"html_url":"https://github.com/acme/backend/issues/42"}"#;
        let created = parse_created(IntegrationKind::GitHub, body).unwrap();
        assert_eq!(created.external_id, "42");
        assert_eq!(
            created.external_url,
            "https://github.com/acme/backend/issues/42"
        );

        let created = parse_created(IntegrationKind::Forgejo, body).unwrap();
        assert_eq!(created.external_id, "42");
    }

    #[test]
    fn parse_created_gitlab_shape() {
        let body = r#"{"iid":7,"web_url":"https://gitlab.example/acme/backend/-/issues/7"}"#;
        let created = parse_created(IntegrationKind::GitLab, body).unwrap();
        assert_eq!(created.external_id, "7");
        assert_eq!(
            created.external_url,
            "https://gitlab.example/acme/backend/-/issues/7"
        );
    }

    #[test]
    fn parse_created_rejects_non_http_scheme_url() {
        let body = r#"{"number":1,"html_url":"javascript:alert(1)"}"#;
        let err = parse_created(IntegrationKind::GitHub, body).unwrap_err();
        assert!(err.to_string().contains("http"));
    }

    #[test]
    fn parse_created_accepts_https_url() {
        let body = r#"{"number":1,"html_url":"https://example.com/issues/1"}"#;
        assert!(parse_created(IntegrationKind::GitHub, body).is_ok());
    }

    #[test]
    fn capped_title_truncates_long_titles() {
        let long = "x".repeat(MAX_TITLE + 50);
        let capped = capped_title(&long);
        assert_eq!(capped.chars().count(), MAX_TITLE);
    }

    #[test]
    fn capped_title_leaves_short_titles_untouched() {
        let short = "a short title";
        assert_eq!(capped_title(short), short);
    }

    /// Accepts one connection, reads the full request (headers + Content-Length
    /// body) so callers can assert on exactly what was sent, writes back the
    /// canned response, then returns the raw captured request bytes.
    async fn respond_once(listener: tokio::net::TcpListener, response: Vec<u8>) -> Vec<u8> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut socket, _) = listener.accept().await.expect("accept loopback conn");
        let mut buf: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = socket.read(&mut chunk).await.expect("read request");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(headers_end) = find_headers_end(&buf) {
                let content_length = parse_content_length(&buf[..headers_end]).unwrap_or(0);
                if buf.len() >= headers_end + content_length {
                    break;
                }
            }
        }
        socket
            .write_all(&response)
            .await
            .expect("write canned response");
        let _ = socket.shutdown().await;
        buf
    }

    fn find_headers_end(buf: &[u8]) -> Option<usize> {
        buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
    }

    fn parse_content_length(headers: &[u8]) -> Option<usize> {
        let text = std::str::from_utf8(headers).ok()?;
        text.split("\r\n").find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().ok())
                .flatten()
        })
    }

    /// A parsed HTTP/1.1 request, used by the round-trip tests to assert on
    /// exactly what `create_issue` sent (method, path, headers, JSON body).
    struct CapturedRequest {
        method: String,
        path: String,
        headers: Vec<(String, String)>,
        body: String,
    }

    impl CapturedRequest {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.as_str())
        }
    }

    fn parse_request(raw: &[u8]) -> CapturedRequest {
        let headers_end = find_headers_end(raw).expect("request should have terminated headers");
        let head = std::str::from_utf8(&raw[..headers_end]).expect("headers should be utf8");
        let body = String::from_utf8_lossy(&raw[headers_end..]).into_owned();

        let mut lines = head.split("\r\n");
        let mut request_line = lines.next().unwrap_or_default().split_whitespace();
        let method = request_line.next().unwrap_or_default().to_string();
        let path = request_line.next().unwrap_or_default().to_string();

        let headers = lines
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
            })
            .collect();

        CapturedRequest {
            method,
            path,
            headers,
            body,
        }
    }

    fn http_response(body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .into_bytes()
    }

    #[tokio::test]
    async fn create_issue_round_trip_github() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback listener");
        let addr = listener.local_addr().expect("local_addr");
        let body = r#"{"number":42,"html_url":"https://github.com/acme/backend/issues/42"}"#;
        let responder = tokio::spawn(respond_once(listener, http_response(body)));

        let target = TrackerTarget {
            base_url: format!("http://{addr}"),
            owner: Some("acme".into()),
            repo: Some("backend".into()),
            project_id: None,
        };
        let client = reqwest::Client::new();
        let created = create_issue(
            &client,
            IntegrationKind::GitHub,
            &target,
            "tok",
            &NewExternalIssue {
                title: "Boom",
                body: "see stackpit",
            },
        )
        .await
        .expect("create_issue should succeed against the loopback server");

        assert_eq!(created.external_id, "42");
        assert_eq!(
            created.external_url,
            "https://github.com/acme/backend/issues/42"
        );

        let raw_request = responder.await.expect("responder task should not panic");
        let request = parse_request(&raw_request);
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/repos/acme/backend/issues");
        assert_eq!(request.header("authorization"), Some("token tok"));
        assert_eq!(request.header("user-agent"), Some("stackpit"));
        let sent: serde_json::Value =
            serde_json::from_str(&request.body).expect("request body should be JSON");
        assert_eq!(sent["title"], "Boom");
        assert_eq!(sent["body"], "see stackpit");
    }

    #[tokio::test]
    async fn create_issue_round_trip_gitlab() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback listener");
        let addr = listener.local_addr().expect("local_addr");
        let body = r#"{"iid":7,"web_url":"https://gitlab.example/acme/backend/-/issues/7"}"#;
        let responder = tokio::spawn(respond_once(listener, http_response(body)));

        let target = TrackerTarget {
            base_url: format!("http://{addr}"),
            owner: None,
            repo: None,
            project_id: Some(9),
        };
        let client = reqwest::Client::new();
        let created = create_issue(
            &client,
            IntegrationKind::GitLab,
            &target,
            "tok",
            &NewExternalIssue {
                title: "Boom",
                body: "see stackpit",
            },
        )
        .await
        .expect("create_issue should succeed against the loopback server");

        assert_eq!(created.external_id, "7");
        assert_eq!(
            created.external_url,
            "https://gitlab.example/acme/backend/-/issues/7"
        );

        let raw_request = responder.await.expect("responder task should not panic");
        let request = parse_request(&raw_request);
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/api/v4/projects/9/issues");
        assert_eq!(request.header("private-token"), Some("tok"));
        let sent: serde_json::Value =
            serde_json::from_str(&request.body).expect("request body should be JSON");
        assert_eq!(sent["title"], "Boom");
        assert_eq!(sent["description"], "see stackpit");
    }

    #[tokio::test]
    async fn read_capped_bails_when_response_exceeds_max() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback listener");
        let addr = listener.local_addr().expect("local_addr");
        let big_body = "x".repeat(1000);
        tokio::spawn(respond_once(listener, http_response(&big_body)));

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/"))
            .send()
            .await
            .expect("send loopback request");

        let result = read_capped(resp, 100).await;
        assert!(result.is_err());
    }
}
