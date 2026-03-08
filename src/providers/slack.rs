use crate::notify::NotificationEvent;
use anyhow::Result;

pub async fn send(client: &reqwest::Client, url: &str, event: &NotificationEvent) -> Result<()> {
    let emoji = match event.level.as_deref() {
        Some("fatal") => ":fire:",
        Some("error") => ":red_circle:",
        Some("warning") => ":warning:",
        Some("info") => ":information_source:",
        Some("debug") => ":mag:",
        _ => ":bell:",
    };

    let trigger_text = match &event.trigger {
        crate::notify::NotifyTrigger::NewIssue => "New Issue".to_string(),
        crate::notify::NotifyTrigger::Regression => "Regression".to_string(),
        crate::notify::NotifyTrigger::ThresholdExceeded {
            count, window_secs, ..
        } => {
            format!("Threshold: {count} events in {}s", window_secs)
        }
        crate::notify::NotifyTrigger::Digest => "Digest".to_string(),
    };

    let title = event.title.as_deref().unwrap_or("(untitled)");

    let payload = if matches!(event.trigger, crate::notify::NotifyTrigger::Digest) {
        // Digest gets its own layout — one section per project
        let mut blocks: Vec<serde_json::Value> = vec![serde_json::json!({
            "type": "header",
            "text": {
                "type": "plain_text",
                "text": format!("{emoji} {trigger_text}"),
                "emoji": true,
            }
        })];

        if let Some(ref digest) = event.digest {
            for project in &digest.projects {
                let name = project.name.as_deref().unwrap_or("Unknown");
                blocks.push(serde_json::json!({
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": format!(
                            "*{}* (project {})\n{} new issues | {} active issues | {} total events",
                            name, project.project_id,
                            project.new_issues.len(), project.active_issues_count, project.total_events
                        ),
                    }
                }));

                for issue in project.new_issues.iter().take(5) {
                    let issue_title = issue.title.as_deref().unwrap_or("(untitled)");
                    let level = issue.level.as_deref().unwrap_or("-");
                    blocks.push(serde_json::json!({
                        "type": "section",
                        "text": {
                            "type": "mrkdwn",
                            "text": format!("  • `{}` [{level}] ({} events)", issue_title, issue.event_count),
                        }
                    }));
                }

                if project.new_issues.len() > 5 {
                    blocks.push(serde_json::json!({
                        "type": "section",
                        "text": {
                            "type": "mrkdwn",
                            "text": format!("  _...and {} more_", project.new_issues.len() - 5),
                        }
                    }));
                }

                blocks.push(serde_json::json!({ "type": "divider" }));
            }
        }

        serde_json::json!({ "blocks": blocks })
    } else {
        serde_json::json!({
            "blocks": [
                {
                    "type": "header",
                    "text": {
                        "type": "plain_text",
                        "text": format!("{emoji} {trigger_text}"),
                        "emoji": true,
                    }
                },
                {
                    "type": "section",
                    "fields": [
                        {
                            "type": "mrkdwn",
                            "text": format!("*Title:*\n{title}"),
                        },
                        {
                            "type": "mrkdwn",
                            "text": format!("*Level:*\n{}", event.level.as_deref().unwrap_or("-")),
                        },
                        {
                            "type": "mrkdwn",
                            "text": format!("*Project:*\n{}", event.project_id),
                        },
                        {
                            "type": "mrkdwn",
                            "text": format!("*Environment:*\n{}", event.environment.as_deref().unwrap_or("-")),
                        },
                    ]
                }
            ]
        })
    };

    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("slack webhook returned {}", resp.status());
    }
    Ok(())
}
