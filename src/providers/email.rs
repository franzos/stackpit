use crate::encoding::escape_html;
use crate::notify::NotificationEvent;
use anyhow::Result;
use polymail::{Address, Body, Email, Mailer};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmailProvider {
    #[default]
    Lettermint,
    Postmark,
    Sendgrid,
}

impl EmailProvider {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "lettermint" => Some(Self::Lettermint),
            "postmark" => Some(Self::Postmark),
            "sendgrid" => Some(Self::Sendgrid),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lettermint => "lettermint",
            Self::Postmark => "postmark",
            Self::Sendgrid => "sendgrid",
        }
    }

    fn mailer(self, token: &str) -> Box<dyn Mailer> {
        match self {
            Self::Lettermint => {
                Box::new(polymail::provider::lettermint::LettermintMailer::new(token))
            }
            Self::Postmark => Box::new(polymail::provider::postmark::PostmarkMailer::new(token)),
            Self::Sendgrid => Box::new(polymail::provider::sendgrid::SendgridMailer::new(token)),
        }
    }
}

pub async fn send(
    email_cfg: &crate::config::EmailConfig,
    secret: Option<&str>,
    integration_config: Option<&str>,
    project_config: Option<&str>,
    event: &NotificationEvent,
) -> Result<()> {
    let int_cfg =
        integration_config.and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok());
    let int_str = |key: &str| {
        int_cfg
            .as_ref()
            .and_then(|v| v.get(key).and_then(|f| f.as_str()).map(String::from))
    };

    let (provider, token, from, name) = if email_cfg.lock {
        (
            email_cfg.provider,
            email_cfg
                .token
                .as_ref()
                .map(|t| t.expose_secret().to_string()),
            email_cfg.from_address.clone(),
            email_cfg.from_name.clone(),
        )
    } else {
        // Absent `provider` means a row predating provider selection -- those are Postmark.
        let provider = int_str("provider")
            .and_then(|p| EmailProvider::parse(&p))
            .unwrap_or(EmailProvider::Postmark);
        let token = secret.map(String::from).or_else(|| {
            email_cfg
                .token
                .as_ref()
                .map(|t| t.expose_secret().to_string())
        });
        let from = int_str("from").or_else(|| email_cfg.from_address.clone());
        let name = int_str("from_name").or_else(|| email_cfg.from_name.clone());
        (provider, token, from, name)
    };

    let token = token.ok_or_else(|| anyhow::anyhow!("email provider token not configured"))?;
    let from = from.ok_or_else(|| anyhow::anyhow!("from address not configured"))?;

    let to = project_config
        .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
        .and_then(|v| v.get("to").and_then(|f| f.as_str()).map(String::from))
        .ok_or_else(|| {
            anyhow::anyhow!("to address not configured in project integration config")
        })?;

    let trigger_text = event.trigger.display_label();

    let title = event.title.as_deref().unwrap_or("(untitled)");
    let level = event.level.as_deref().unwrap_or("-");
    let env = event.environment.as_deref().unwrap_or("-");
    let subject = format!("[Stackpit] {trigger_text}: {title}");

    let (text_body, html_body) = if matches!(event.trigger, crate::notify::NotifyTrigger::Digest) {
        let mut text = format!("{trigger_text}\n\n");
        let mut html = format!(
            r#"<div style="font-family: -apple-system, system-ui, sans-serif; max-width: 600px;">
<h2 style="color: #333;">{}</h2>"#,
            escape_html(&trigger_text)
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
                    escape_html(name), escape_html(&project.project_id.to_string()),
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
                        escape_html(t), escape_html(l), issue.event_count
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
            escape_html(&trigger_text),
            escape_html(title),
            escape_html(level),
            escape_html(&event.project_id.to_string()),
            escape_html(env),
            escape_html(&event.event_id.to_string()),
        );
        (text, html)
    };

    let from_addr = match name {
        Some(n) if !n.trim().is_empty() => Address::with_name(from, n),
        _ => Address::new(from),
    };

    let email = Email::builder(
        from_addr,
        subject,
        Body::Both {
            html: html_body,
            text: text_body,
        },
    )
    .to(to)
    .build()
    .map_err(|e| anyhow::anyhow!("failed to build email: {e}"))?;

    provider
        .mailer(&token)
        .send(&email)
        .await
        .map_err(|e| anyhow::anyhow!("{} send failed: {e}", provider.as_str()))?;
    Ok(())
}
