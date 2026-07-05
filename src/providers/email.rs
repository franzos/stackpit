use crate::notify::NotificationEvent;
use crate::util::encoding::escape_html;
use anyhow::Result;
use polymail::provider::smtp::{SmtpMailer, SmtpTls};
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
    /// Instance-wide SMTP relay. Unlike the API providers, its credential is a
    /// connection block (`[email.smtp]`) rather than a single per-integration
    /// token, so it's always driven by the global config.
    Smtp,
}

impl EmailProvider {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "lettermint" => Some(Self::Lettermint),
            "postmark" => Some(Self::Postmark),
            "sendgrid" => Some(Self::Sendgrid),
            "smtp" => Some(Self::Smtp),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lettermint => "lettermint",
            Self::Postmark => "postmark",
            Self::Sendgrid => "sendgrid",
            Self::Smtp => "smtp",
        }
    }

    /// True for the API providers, whose credential is a single token supplied
    /// per integration or globally. SMTP is the exception: it's configured once
    /// via `[email.smtp]`, so it carries no per-integration token.
    pub fn is_token_based(self) -> bool {
        !matches!(self, Self::Smtp)
    }
}

/// Transport security for an SMTP relay, mirroring polymail's `SmtpTls`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmtpTlsMode {
    /// Plaintext, no encryption (local sinks like mailcrab/MailSlurper on 1025).
    None,
    /// Connect plaintext then upgrade via STARTTLS (usually port 587).
    Starttls,
    /// TLS from the first byte (usually port 465). Secure default.
    #[default]
    Implicit,
}

impl SmtpTlsMode {
    fn to_polymail(self) -> SmtpTls {
        match self {
            Self::None => SmtpTls::None,
            Self::Starttls => SmtpTls::StartTls,
            Self::Implicit => SmtpTls::Implicit,
        }
    }
}

/// Builds the polymail mailer for `provider`. API providers need a token; SMTP
/// is built from the global `[email.smtp]` connection block instead.
fn build_mailer(
    provider: EmailProvider,
    token: Option<&str>,
    smtp: &crate::config::SmtpConfig,
) -> Result<Box<dyn Mailer>> {
    let token = || token.ok_or_else(|| anyhow::anyhow!("email provider token not configured"));
    Ok(match provider {
        EmailProvider::Lettermint => Box::new(
            polymail::provider::lettermint::LettermintMailer::new(token()?),
        ),
        EmailProvider::Postmark => {
            Box::new(polymail::provider::postmark::PostmarkMailer::new(token()?))
        }
        EmailProvider::Sendgrid => {
            Box::new(polymail::provider::sendgrid::SendgridMailer::new(token()?))
        }
        EmailProvider::Smtp => Box::new(build_smtp_mailer(smtp)?),
    })
}

fn build_smtp_mailer(cfg: &crate::config::SmtpConfig) -> Result<SmtpMailer> {
    let host = cfg
        .host
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("SMTP host not configured; set [email.smtp] host"))?;
    let mut builder = SmtpMailer::builder(host).tls(cfg.tls.to_polymail());
    // 0 means "unset": fall back to lettre's per-TLS default port (465/587/25).
    if let Some(port) = cfg.port.filter(|p| *p != 0) {
        builder = builder.port(port);
    }
    if let Some(user) = cfg.username.as_deref().filter(|u| !u.is_empty()) {
        let pass = cfg
            .password
            .as_ref()
            .map(|p| p.expose_secret().to_string())
            .unwrap_or_default();
        builder = builder.credentials(user, pass);
    }
    builder
        .build()
        .map_err(|e| anyhow::anyhow!("invalid [email.smtp] config: {e}"))
}

pub async fn send(
    email_cfg: &crate::config::EmailConfig,
    base_url: &str,
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

    let from = from.ok_or_else(|| anyhow::anyhow!("from address not configured"))?;

    let to = project_config
        .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
        .and_then(|v| v.get("to").and_then(|f| f.as_str()).map(String::from))
        .ok_or_else(|| {
            anyhow::anyhow!("to address not configured in project integration config")
        })?;

    let base = base_url.trim_end_matches('/');
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

        if event.digest.as_ref().is_some_and(|d| d.sample) {
            text.push_str("(SAMPLE) No activity in this period; showing example data.\n\n");
            html.push_str(
                r#"<p style="padding: 8px 12px; background: #fff3cd; color: #664d03; border-radius: 4px;"><strong>Sample digest</strong> — no activity in this period; showing example data.</p>"#,
            );
        }

        if let Some(ref digest) = event.digest {
            for project in &digest.projects {
                let name = project.name.as_deref().unwrap_or("Unknown");
                let project_url = format!("{base}/web/projects/{}/", project.project_id);
                text.push_str(&format!(
                    "\n{} (project {})\n  {} new issues | {} active | {} events\n  {}\n",
                    name,
                    project.project_id,
                    project.new_issues.len(),
                    project.active_issues_count,
                    project.total_events,
                    project_url,
                ));
                html.push_str(&format!(
                    r#"<h3 style="margin-top: 16px;"><a href="{}" style="color: #4f46e5; text-decoration: none;">{} (project {})</a></h3>
<p>{} new issues | {} active issues | {} total events</p>
<table style="border-collapse: collapse; width: 100%;">
<tr><th style="padding: 8px; border-bottom: 2px solid #ddd; text-align: left;">Title</th><th style="padding: 8px; border-bottom: 2px solid #ddd; text-align: left;">Level</th><th style="padding: 8px; border-bottom: 2px solid #ddd; text-align: right;">Events</th></tr>"#,
                    escape_html(&project_url), escape_html(name), escape_html(&project.project_id.to_string()),
                    project.new_issues.len(), project.active_issues_count, project.total_events
                ));

                for issue in &project.new_issues {
                    let t = issue.title.as_deref().unwrap_or("(untitled)");
                    let l = issue.level.as_deref().unwrap_or("-");
                    // Sample/synthetic issues carry no fingerprint; render them as
                    // plain text so a preview never emits a link that 404s.
                    let issue_url = (!issue.fingerprint.is_empty()).then(|| {
                        format!(
                            "{base}/web/projects/{}/issues/{}/",
                            project.project_id, issue.fingerprint
                        )
                    });
                    match &issue_url {
                        Some(url) => text.push_str(&format!(
                            "  - {} [{}] ({} events)\n    {}\n",
                            t, l, issue.event_count, url,
                        )),
                        None => text.push_str(&format!(
                            "  - {} [{}] ({} events)\n",
                            t, l, issue.event_count,
                        )),
                    }
                    let title_cell = match &issue_url {
                        Some(url) => format!(
                            r#"<a href="{}" style="color: #4f46e5; text-decoration: none;">{}</a>"#,
                            escape_html(url),
                            escape_html(t)
                        ),
                        None => escape_html(t),
                    };
                    html.push_str(&format!(
                        r#"<tr><td style="padding: 8px; border-bottom: 1px solid #eee;">{}</td><td style="padding: 8px; border-bottom: 1px solid #eee;">{}</td><td style="padding: 8px; border-bottom: 1px solid #eee; text-align: right;">{}</td></tr>"#,
                        title_cell, escape_html(l), issue.event_count
                    ));
                }
                html.push_str("</table>");
            }
        }

        html.push_str("</div>");
        (text, html)
    } else {
        // Synthetic events (Test button) carry no fingerprint; skip the link so
        // it can't 404. Real notifications always have one.
        let (text_link, html_link) = if event.fingerprint.is_empty() {
            (String::new(), String::new())
        } else {
            let issue_url = format!(
                "{base}/web/projects/{}/issues/{}/",
                event.project_id, event.fingerprint
            );
            (
                format!("\n\nView issue: {issue_url}"),
                format!(
                    r#"<p style="margin-top: 16px;"><a href="{}" style="display: inline-block; padding: 10px 16px; background: #4f46e5; color: #ffffff; text-decoration: none; border-radius: 6px; font-weight: 600;">View issue in Stackpit</a></p>"#,
                    escape_html(&issue_url)
                ),
            )
        };
        let text = format!(
            "{trigger_text}\n\nTitle: {title}\nLevel: {level}\nProject: {}\nEnvironment: {env}\nEvent ID: {}{text_link}",
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
{html_link}
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

    build_mailer(provider, token.as_deref(), &email_cfg.smtp)?
        .send(&email)
        .await
        .map_err(|e| anyhow::anyhow!("{} send failed: {e}", provider.as_str()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SmtpConfig;

    #[test]
    fn parse_and_token_classification() {
        assert_eq!(EmailProvider::parse("smtp"), Some(EmailProvider::Smtp));
        assert_eq!(EmailProvider::Smtp.as_str(), "smtp");
        assert!(!EmailProvider::Smtp.is_token_based());
        assert!(EmailProvider::Postmark.is_token_based());
    }

    #[test]
    fn smtp_mailer_requires_host() {
        assert!(build_smtp_mailer(&SmtpConfig::default()).is_err());
    }

    // Building the transport constructs a lettre connection pool, which needs a
    // Tokio runtime in scope (as it does on the real dispatch path).
    #[tokio::test]
    async fn smtp_plaintext_sink_builds() {
        let cfg = SmtpConfig {
            host: Some("localhost".into()),
            port: Some(1025),
            tls: SmtpTlsMode::None,
            username: None,
            password: None,
        };
        assert!(build_smtp_mailer(&cfg).is_ok());
    }

    #[test]
    fn smtp_credentials_over_plaintext_rejected_by_polymail() {
        use secrecy::SecretString;
        let cfg = SmtpConfig {
            host: Some("localhost".into()),
            port: Some(1025),
            tls: SmtpTlsMode::None,
            username: Some("user".into()),
            password: Some(SecretString::from("pass")),
        };
        assert!(build_smtp_mailer(&cfg).is_err());
    }

    // Real delivery against a local plaintext sink (mailcrab/MailHog on 1025),
    // driving the actual send() path end to end. Ignored by default; run with
    // `cargo test -- --ignored smtp_send_delivers` while a sink is listening.
    #[tokio::test]
    #[ignore = "requires an SMTP sink on localhost:1025"]
    async fn smtp_send_delivers_to_local_sink() {
        let email_cfg = crate::config::EmailConfig {
            provider: EmailProvider::Smtp,
            from_address: Some("alerts@stackpit.test".into()),
            from_name: Some("Stackpit".into()),
            token: None,
            lock: true,
            smtp: SmtpConfig {
                host: Some("localhost".into()),
                port: Some(1025),
                tls: SmtpTlsMode::None,
                username: None,
                password: None,
            },
        };
        let event = NotificationEvent {
            trigger: crate::notify::NotifyTrigger::NewIssue,
            project_id: 42,
            fingerprint: "smtp-live-fingerprint".into(),
            title: Some("SMTP live delivery check".into()),
            level: Some("error".into()),
            environment: Some("test".into()),
            event_id: "smtp-live-event".into(),
            digest: None,
        };
        super::send(
            &email_cfg,
            "http://localhost:3000",
            None,
            None,
            Some(r#"{"to":"smtp-live-test@stackpit.test"}"#),
            &event,
        )
        .await
        .expect("SMTP send to local sink should succeed");
    }
}
