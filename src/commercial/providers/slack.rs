use crate::notify::NotificationEvent;
use anyhow::Result;

/// Escape Slack mrkdwn control characters so attacker-controlled fields (event
/// title, environment, project/issue name) can't inject a `<url|text>` link into
/// an operator's alert channel. `&` must be replaced first to avoid double-escaping.
fn escape_mrkdwn(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub async fn send(client: &reqwest::Client, url: &str, event: &NotificationEvent) -> Result<()> {
    let emoji = match event.level.as_deref() {
        Some("fatal") => ":fire:",
        Some("error") => ":red_circle:",
        Some("warning") => ":warning:",
        Some("info") => ":information_source:",
        Some("debug") => ":mag:",
        _ => ":bell:",
    };

    let trigger_text = event.trigger.display_label();

    let title = escape_mrkdwn(event.title.as_deref().unwrap_or("(untitled)"));

    let payload = if matches!(event.trigger, crate::notify::NotifyTrigger::Digest) {
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
                let name = escape_mrkdwn(project.name.as_deref().unwrap_or("Unknown"));
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
                    // Backticks stripped so the title can't break out of the code span.
                    let issue_title = escape_mrkdwn(issue.title.as_deref().unwrap_or("(untitled)"))
                        .replace('`', "'");
                    let level = escape_mrkdwn(issue.level.as_deref().unwrap_or("-"));
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
                            "text": format!("*Level:*\n{}", escape_mrkdwn(event.level.as_deref().unwrap_or("-"))),
                        },
                        {
                            "type": "mrkdwn",
                            "text": format!("*Project:*\n{}", event.project_id),
                        },
                        {
                            "type": "mrkdwn",
                            "text": format!("*Environment:*\n{}", escape_mrkdwn(event.environment.as_deref().unwrap_or("-"))),
                        },
                    ]
                }
            ]
        })
    };

    let req = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&payload);

    super::send_and_check(req, "slack webhook").await
}
