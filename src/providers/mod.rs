pub mod email;
pub mod slack;
pub mod webhook;

use crate::notify::NotificationEvent;
use anyhow::Result;

/// Dispatches HTTP-based integrations. Email is handled separately at the call
/// sites: it needs the global mailer config and has no client/url/SSRF surface.
pub async fn dispatch(
    client: &reqwest::Client,
    kind: &str,
    url: &str,
    secret: Option<&str>,
    event: &NotificationEvent,
) -> Result<()> {
    match kind {
        "webhook" => webhook::send(client, url, secret, event).await,
        "slack" => slack::send(client, url, event).await,
        other => anyhow::bail!("unknown integration kind: {other}"),
    }
}
