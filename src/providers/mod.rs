pub mod email;
pub mod slack;
pub mod webhook;

use crate::notify::NotificationEvent;
use anyhow::Result;

pub async fn dispatch(
    client: &reqwest::Client,
    kind: &str,
    url: &str,
    secret: Option<&str>,
    integration_config: Option<&str>,
    project_config: Option<&str>,
    event: &NotificationEvent,
) -> Result<()> {
    match kind {
        "webhook" => webhook::send(client, url, secret, event).await,
        "slack" => slack::send(client, url, event).await,
        "email" => {
            email::send(
                client,
                url,
                secret,
                integration_config,
                project_config,
                event,
            )
            .await
        }
        other => anyhow::bail!("unknown integration kind: {other}"),
    }
}
