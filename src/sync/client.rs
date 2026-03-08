use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

pub struct SentryClient {
    http: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Deserialize)]
pub struct SentryProject {
    #[serde(deserialize_with = "deserialize_id")]
    pub id: u64,
    pub slug: String,
    pub name: String,
    pub platform: Option<String>,
}

#[derive(Debug)]
pub struct SentryEvent {
    pub json: Value,
}

impl SentryEvent {
    pub fn timestamp(&self) -> Option<i64> {
        // dateCreated comes back as ISO-8601 from the API
        self.json
            .get("dateCreated")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp())
    }
}

pub struct EventPage {
    pub events: Vec<SentryEvent>,
    pub next_cursor: Option<String>,
    pub has_next: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SentryIssue {
    pub id: String,
    pub status: String,
    #[serde(rename = "shortId")]
    pub short_id: String,
    pub title: String,
    pub level: Option<String>,
    #[serde(rename = "firstSeen")]
    pub first_seen: Option<String>,
    #[serde(rename = "lastSeen")]
    pub last_seen: Option<String>,
    pub count: Option<String>,
    pub project: SentryIssueProject,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SentryIssueProject {
    pub id: String,
    pub slug: String,
}

#[derive(Debug, Deserialize)]
pub struct SentryProjectKey {
    #[serde(rename = "public")]
    pub public_key: String,
    #[serde(rename = "isActive")]
    pub is_active: bool,
    pub label: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SentryAttachmentInfo {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub attachment_type: Option<String>,
    pub mimetype: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SentryRelease {
    pub version: String,
    #[serde(rename = "dateCreated")]
    pub date_created: Option<String>,
    #[serde(rename = "dateReleased")]
    pub date_released: Option<String>,
    #[serde(rename = "firstEvent")]
    pub first_event: Option<String>,
    #[serde(rename = "lastEvent")]
    pub last_event: Option<String>,
    #[serde(rename = "newGroups")]
    pub new_groups: Option<u64>,
    #[serde(rename = "lastCommit")]
    pub last_commit: Option<SentryCommit>,
}

#[derive(Debug, Deserialize)]
pub struct SentryCommit {
    pub id: Option<String>,
}

pub struct IssuePage {
    pub issues: Vec<SentryIssue>,
    pub next_cursor: Option<String>,
    pub has_next: bool,
}

impl SentryClient {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
                        .map_err(|e| anyhow::anyhow!("invalid auth token: {e}"))?,
                );
                headers
            })
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build HTTP client: {e}"))?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub async fn list_projects(&self, org: &str) -> Result<Vec<SentryProject>> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut url = format!("{}/api/0/organizations/{}/projects/", self.base_url, org);

            if let Some(ref c) = cursor {
                url.push_str(&format!("?cursor={c}"));
            }

            let resp = self.get_with_retry(&url).await?;
            let (next_cursor, has_next) = parse_link_header(resp.headers());
            let projects: Vec<SentryProject> = resp.json().await?;

            if projects.is_empty() {
                break;
            }

            all.extend(projects);

            if !has_next {
                break;
            }

            cursor = next_cursor;
        }

        Ok(all)
    }

    pub async fn list_project_keys(
        &self,
        org: &str,
        project_slug: &str,
    ) -> Result<Vec<SentryProjectKey>> {
        let url = format!(
            "{}/api/0/projects/{}/{}/keys/",
            self.base_url, org, project_slug
        );
        let resp = self.get_with_retry(&url).await?;
        let keys: Vec<SentryProjectKey> = resp.json().await?;
        Ok(keys)
    }

    pub async fn list_events(
        &self,
        org: &str,
        project_slug: &str,
        start: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<EventPage> {
        let mut url = format!(
            "{}/api/0/projects/{}/{}/events/?full=true",
            self.base_url, org, project_slug
        );

        if let Some(start) = start {
            url.push_str(&format!("&start={start}"));
        }

        if let Some(cursor) = cursor {
            url.push_str(&format!("&cursor={cursor}"));
        }

        let resp = self.get_with_retry(&url).await?;
        let (next_cursor, has_next) = parse_link_header(resp.headers());
        let events_json: Vec<Value> = resp.json().await?;

        let events = events_json
            .into_iter()
            .map(|json| SentryEvent { json })
            .collect();

        Ok(EventPage {
            events,
            next_cursor,
            has_next,
        })
    }

    /// Fetches issues for an org, filtered by project ID.
    /// We pass `query=` (empty) to get all statuses — not just unresolved.
    pub async fn list_issues(
        &self,
        org: &str,
        project_id: u64,
        cursor: Option<&str>,
    ) -> Result<IssuePage> {
        let mut url = format!(
            "{}/api/0/organizations/{}/issues/?query=&project={}",
            self.base_url, org, project_id
        );

        if let Some(cursor) = cursor {
            url.push_str(&format!("&cursor={cursor}"));
        }

        let resp = self.get_with_retry(&url).await?;
        let (next_cursor, has_next) = parse_link_header(resp.headers());
        let issues: Vec<SentryIssue> = resp.json().await?;

        Ok(IssuePage {
            issues,
            next_cursor,
            has_next,
        })
    }

    /// Fetches the attachment list for a given event.
    pub async fn list_event_attachments(
        &self,
        org: &str,
        project_slug: &str,
        event_id: &str,
    ) -> Result<Vec<SentryAttachmentInfo>> {
        let url = format!(
            "{}/api/0/projects/{}/{}/events/{}/attachments/",
            self.base_url, org, project_slug, event_id
        );

        let resp = self.get_with_retry(&url).await?;
        let attachments: Vec<SentryAttachmentInfo> = resp.json().await?;
        Ok(attachments)
    }

    /// Downloads the raw binary data for an attachment.
    pub async fn download_attachment(
        &self,
        org: &str,
        project_slug: &str,
        event_id: &str,
        attachment_id: &str,
    ) -> Result<Vec<u8>> {
        let url = format!(
            "{}/api/0/projects/{}/{}/events/{}/attachments/{}/?download=1",
            self.base_url, org, project_slug, event_id, attachment_id
        );

        let resp = self.get_with_retry(&url).await?;
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// Fetches releases for an org, optionally scoped to a specific project.
    pub async fn list_releases(
        &self,
        org: &str,
        project_id: Option<u64>,
        cursor: Option<&str>,
    ) -> Result<(Vec<SentryRelease>, Option<String>, bool)> {
        let mut url = format!("{}/api/0/organizations/{}/releases/", self.base_url, org);

        let mut has_param = false;
        if let Some(pid) = project_id {
            url.push_str(&format!("?project={pid}"));
            has_param = true;
        }
        if let Some(cursor) = cursor {
            url.push_str(if has_param { "&" } else { "?" });
            url.push_str(&format!("cursor={cursor}"));
        }

        let resp = self.get_with_retry(&url).await?;
        let (next_cursor, has_next) = parse_link_header(resp.headers());
        let releases: Vec<SentryRelease> = resp.json().await?;

        Ok((releases, next_cursor, has_next))
    }

    async fn get_with_retry(&self, url: &str) -> Result<reqwest::Response> {
        let mut delay = Duration::from_secs(1);
        let max_retries = 5;

        for attempt in 0..=max_retries {
            let resp = match self.http.get(url).send().await {
                Ok(r) => r,
                Err(e) if (e.is_timeout() || e.is_connect()) && attempt < max_retries => {
                    tracing::warn!("network error: {e}, retry {attempt}/{max_retries}");
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(60));
                    continue;
                }
                Err(e) => return Err(e.into()),
            };

            match resp.status().as_u16() {
                429 => {
                    let wait = resp
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .map(Duration::from_secs)
                        .unwrap_or(delay);
                    tracing::warn!("rate limited, waiting {wait:?}");
                    tokio::time::sleep(wait).await;
                    delay = (delay * 2).min(Duration::from_secs(60));
                    continue;
                }
                500..=599 if attempt < max_retries => {
                    tracing::warn!(
                        "server error {}, retry {attempt}/{max_retries}",
                        resp.status()
                    );
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(60));
                    continue;
                }
                200..=299 => return Ok(resp),
                status => {
                    let body = resp.text().await.unwrap_or_default();
                    bail!("API error {status}: {body}");
                }
            }
        }

        bail!("max retries exceeded for {url}")
    }
}

/// Parses Sentry's `Link` header for cursor-based pagination. It turns out
/// they pack the cursor and a `results` flag into the header — quite an unusual choice.
fn parse_link_header(headers: &reqwest::header::HeaderMap) -> (Option<String>, bool) {
    let header = match headers.get("link").and_then(|v| v.to_str().ok()) {
        Some(h) => h,
        None => return (None, false),
    };

    // Format: <url>; rel="next"; results="true"; cursor="0:100:0", ...
    for part in header.split(',') {
        let part = part.trim();
        if !part.contains("rel=\"next\"") {
            continue;
        }

        let has_results = part.contains("results=\"true\"");

        let cursor = part.split(';').find_map(|segment| {
            let segment = segment.trim();
            if segment.starts_with("cursor=\"") {
                Some(
                    segment
                        .trim_start_matches("cursor=\"")
                        .trim_end_matches('"')
                        .to_string(),
                )
            } else {
                None
            }
        });

        return (cursor, has_results);
    }

    (None, false)
}

/// Sentry can't make up its mind — IDs come back as strings in some
/// deployments and numbers in others. This handles both.
fn deserialize_id<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    use serde::de;

    struct IdVisitor;
    impl<'de> de::Visitor<'de> for IdVisitor {
        type Value = u64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a u64 or a string containing a u64")
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u64, E> {
            Ok(v)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<u64, E> {
            v.parse().map_err(de::Error::custom)
        }
    }
    deserializer.deserialize_any(IdVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_link_header_with_next() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "link",
            reqwest::header::HeaderValue::from_static(
                r#"<https://sentry.io/api/0/projects/org/proj/events/?cursor=0:100:0>; rel="next"; results="true"; cursor="0:100:0", <https://sentry.io/api/0/projects/org/proj/events/?cursor=0:0:1>; rel="previous"; results="false"; cursor="0:0:1""#,
            ),
        );

        let (cursor, has_next) = parse_link_header(&headers);
        assert_eq!(cursor.as_deref(), Some("0:100:0"));
        assert!(has_next);
    }

    #[test]
    fn parse_link_header_no_more_results() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "link",
            reqwest::header::HeaderValue::from_static(
                r#"<https://sentry.io/api/0/x/?cursor=0:200:0>; rel="next"; results="false"; cursor="0:200:0""#,
            ),
        );

        let (cursor, has_next) = parse_link_header(&headers);
        assert_eq!(cursor.as_deref(), Some("0:200:0"));
        assert!(!has_next);
    }

    #[test]
    fn parse_link_header_missing() {
        let headers = reqwest::header::HeaderMap::new();
        let (cursor, has_next) = parse_link_header(&headers);
        assert!(cursor.is_none());
        assert!(!has_next);
    }
}
