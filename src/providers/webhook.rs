use crate::notify::NotificationEvent;
use anyhow::Result;
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub async fn send(
    client: &reqwest::Client,
    url: &str,
    secret: Option<&str>,
    event: &NotificationEvent,
) -> Result<()> {
    let mut payload = serde_json::json!({
        "trigger": event.trigger.as_str(),
        "project_id": event.project_id,
        "fingerprint": event.fingerprint,
        "title": event.title,
        "level": event.level,
        "environment": event.environment,
        "event_id": event.event_id,
    });

    // Tack on fields specific to the trigger type
    match &event.trigger {
        crate::notify::NotifyTrigger::ThresholdExceeded {
            rule_id,
            count,
            window_secs,
        } => {
            payload["threshold"] = serde_json::json!({
                "rule_id": rule_id,
                "count": count,
                "window_secs": window_secs,
            });
        }
        crate::notify::NotifyTrigger::Digest => {
            if let Some(ref digest) = event.digest {
                let projects: Vec<serde_json::Value> = digest
                    .projects
                    .iter()
                    .map(|p| {
                        let issues: Vec<serde_json::Value> = p
                            .new_issues
                            .iter()
                            .map(|i| {
                                serde_json::json!({
                                    "fingerprint": i.fingerprint,
                                    "title": i.title,
                                    "level": i.level,
                                    "event_count": i.event_count,
                                    "first_seen": i.first_seen,
                                })
                            })
                            .collect();
                        serde_json::json!({
                            "project_id": p.project_id,
                            "name": p.name,
                            "new_issues": issues,
                            "active_issues_count": p.active_issues_count,
                            "total_events": p.total_events,
                        })
                    })
                    .collect();
                payload["digest"] = serde_json::json!({
                    "period_start": digest.period_start,
                    "period_end": digest.period_end,
                    "projects": projects,
                });
            }
        }
        _ => {}
    }

    let body = serde_json::to_vec(&payload)?;

    let mut req = client
        .post(url)
        .header("Content-Type", "application/json")
        .body(body.clone());

    if let Some(secret) = secret {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key size");
        mac.update(&body);
        let signature = hex::encode(mac.finalize().into_bytes());
        req = req.header("X-Stackpit-Signature", signature);
    } else {
        tracing::warn!(
            "webhook to {url} sent without signature — configure a secret for HMAC verification"
        );
    }

    let resp = req.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("webhook returned {}", resp.status());
    }
    Ok(())
}
