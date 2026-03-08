use crate::notify::NotificationEvent;
use anyhow::Result;

/// Escape characters that have special meaning in HTML to prevent XSS
/// when interpolating user-controlled values into HTML email bodies.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

pub async fn send(
    client: &reqwest::Client,
    url: &str,
    secret: Option<&str>,
    integration_config: Option<&str>,
    project_config: Option<&str>,
    event: &NotificationEvent,
) -> Result<()> {
    let token = secret.ok_or_else(|| anyhow::anyhow!("Postmark server token not configured"))?;

    let from = integration_config
        .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
        .and_then(|v| v.get("from").and_then(|f| f.as_str()).map(String::from))
        .ok_or_else(|| anyhow::anyhow!("from address not configured in integration config"))?;

    let to = project_config
        .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
        .and_then(|v| v.get("to").and_then(|f| f.as_str()).map(String::from))
        .ok_or_else(|| {
            anyhow::anyhow!("to address not configured in project integration config")
        })?;

    let trigger_text = match &event.trigger {
        crate::notify::NotifyTrigger::NewIssue => "New Issue".to_string(),
        crate::notify::NotifyTrigger::Regression => "Regression".to_string(),
        crate::notify::NotifyTrigger::ThresholdExceeded {
            count, window_secs, ..
        } => {
            format!("Threshold: {} events in {}s", count, window_secs)
        }
        crate::notify::NotifyTrigger::Digest => "Digest".to_string(),
    };

    let title = event.title.as_deref().unwrap_or("(untitled)");
    let level = event.level.as_deref().unwrap_or("-");
    let env = event.environment.as_deref().unwrap_or("-");
    let subject = format!("[Stackpit] {trigger_text}: {title}");

    let (text_body, html_body) = if matches!(event.trigger, crate::notify::NotifyTrigger::Digest) {
        let mut text = format!("{trigger_text}\n\n");
        let mut html = format!(
            r#"<div style="font-family: -apple-system, system-ui, sans-serif; max-width: 600px;">
<h2 style="color: #333;">{}</h2>"#,
            html_escape(&trigger_text)
        );

        if let Some(ref digest) = event.digest {
            for project in &digest.projects {
                let name = project.name.as_deref().unwrap_or("Unknown");
                text.push_str(&format!(
                    "\n{} (project {})\n  {} new issues | {} active | {} events\n",
                    name,
                    project.project_id,
                    project.new_issues.len(),
                    project.active_issues_count,
                    project.total_events
                ));
                html.push_str(&format!(
                    r#"<h3 style="margin-top: 16px;">{} (project {})</h3>
<p>{} new issues | {} active issues | {} total events</p>
<table style="border-collapse: collapse; width: 100%;">
<tr><th style="padding: 8px; border-bottom: 2px solid #ddd; text-align: left;">Title</th><th style="padding: 8px; border-bottom: 2px solid #ddd; text-align: left;">Level</th><th style="padding: 8px; border-bottom: 2px solid #ddd; text-align: right;">Events</th></tr>"#,
                    html_escape(name), html_escape(&project.project_id.to_string()),
                    project.new_issues.len(), project.active_issues_count, project.total_events
                ));

                for issue in &project.new_issues {
                    let t = issue.title.as_deref().unwrap_or("(untitled)");
                    let l = issue.level.as_deref().unwrap_or("-");
                    text.push_str(&format!(
                        "  - {} [{}] ({} events)\n",
                        t, l, issue.event_count
                    ));
                    html.push_str(&format!(
                        r#"<tr><td style="padding: 8px; border-bottom: 1px solid #eee;">{}</td><td style="padding: 8px; border-bottom: 1px solid #eee;">{}</td><td style="padding: 8px; border-bottom: 1px solid #eee; text-align: right;">{}</td></tr>"#,
                        html_escape(t), html_escape(l), issue.event_count
                    ));
                }
                html.push_str("</table>");
            }
        }

        html.push_str("</div>");
        (text, html)
    } else {
        let text = format!(
            "{trigger_text}\n\nTitle: {title}\nLevel: {level}\nProject: {}\nEnvironment: {env}\nEvent ID: {}",
            event.project_id, event.event_id,
        );
        let html = format!(
            r#"<div style="font-family: -apple-system, system-ui, sans-serif; max-width: 600px;">
<h2 style="color: #333;">{}</h2>
<table style="border-collapse: collapse; width: 100%;">
<tr><td style="padding: 8px; border-bottom: 1px solid #eee; font-weight: bold;">Title</td><td style="padding: 8px; border-bottom: 1px solid #eee;">{}</td></tr>
<tr><td style="padding: 8px; border-bottom: 1px solid #eee; font-weight: bold;">Level</td><td style="padding: 8px; border-bottom: 1px solid #eee;">{}</td></tr>
<tr><td style="padding: 8px; border-bottom: 1px solid #eee; font-weight: bold;">Project</td><td style="padding: 8px; border-bottom: 1px solid #eee;">{}</td></tr>
<tr><td style="padding: 8px; border-bottom: 1px solid #eee; font-weight: bold;">Environment</td><td style="padding: 8px; border-bottom: 1px solid #eee;">{}</td></tr>
<tr><td style="padding: 8px; font-weight: bold;">Event ID</td><td style="padding: 8px;">{}</td></tr>
</table>
</div>"#,
            html_escape(&trigger_text),
            html_escape(title),
            html_escape(level),
            html_escape(&event.project_id.to_string()),
            html_escape(env),
            html_escape(&event.event_id.to_string()),
        );
        (text, html)
    };

    let payload = serde_json::json!({
        "From": from,
        "To": to,
        "Subject": subject,
        "TextBody": text_body,
        "HtmlBody": html_body,
    });

    let resp = client
        .post(url)
        .header("X-Postmark-Server-Token", token)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Postmark returned {status}: {body}");
    }
    Ok(())
}
