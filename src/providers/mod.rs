pub mod email;

use crate::commercial::providers::{slack, webhook};
use crate::commercial::LicenseHandle;
use crate::domain::IntegrationKind;
use crate::notify::NotificationEvent;
use anyhow::Result;

/// Dispatches HTTP-based integrations. Email is handled separately at the call
/// sites: it needs the global mailer config and has no client/url/SSRF surface.
///
/// Takes the license handle rather than trusting callers to pre-check, so a new
/// call site can't skip the delivery gate. Refusal is an `Err`, never a silent
/// drop — callers log it or surface it as a flash.
pub async fn dispatch(
    license: &LicenseHandle,
    client: &reqwest::Client,
    kind: &IntegrationKind,
    url: &str,
    secret: Option<&str>,
    event: &NotificationEvent,
) -> Result<()> {
    if !crate::commercial::providers::may_deliver(license, *kind) {
        anyhow::bail!(
            "{} integrations require an active commercial license",
            kind.as_str()
        );
    }
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
