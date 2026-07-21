pub mod email;
pub mod slack;
pub mod tracker;
pub mod webhook;

use crate::domain::IntegrationKind;
use crate::notify::NotificationEvent;
use anyhow::Result;

/// Send a built request and bail if the response status isn't 2xx. `label`
/// names the provider in the error (e.g. "webhook", "slack webhook").
pub(crate) async fn send_and_check(req: reqwest::RequestBuilder, label: &str) -> Result<()> {
    let resp = req.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("{label} returned {}", resp.status());
    }
    Ok(())
}

/// Dispatches HTTP-based integrations. Email is handled separately at the call
/// sites: it needs the global mailer config and has no client/url/SSRF surface.
pub async fn dispatch(
    client: &reqwest::Client,
    kind: &IntegrationKind,
    url: &str,
    secret: Option<&str>,
    event: &NotificationEvent,
) -> Result<()> {
    match kind {
        IntegrationKind::Webhook => webhook::send(client, url, secret, event).await,
        IntegrationKind::Slack => slack::send(client, url, event).await,
        IntegrationKind::Email => {
            anyhow::bail!("email integrations are dispatched separately, not via dispatch()")
        }
        IntegrationKind::GitHub | IntegrationKind::Forgejo | IntegrationKind::GitLab => {
            // Trackers are not notification channels; issue creation is an explicit user action.
            anyhow::bail!(
                "integration kind {} is not a notification provider",
                kind.as_str()
            )
        }
    }
}
