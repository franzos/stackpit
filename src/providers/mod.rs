pub mod email;
pub mod slack;
pub mod webhook;

use crate::notify::NotificationEvent;
use crate::queries::types::IntegrationKind;
use anyhow::Result;

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
    }
}
