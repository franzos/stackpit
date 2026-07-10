use crate::config::EmailConfig;
use crate::notify::NotificationEvent;
use crate::util::encoding::escape_html;
use anyhow::Result;
use polymail::{Address, Body, Email, ProviderConfig};

/// The provider tag as it appears in stored integration JSON and the UI form.
pub fn provider_label(cfg: &ProviderConfig) -> &'static str {
    match cfg {
        ProviderConfig::Lettermint { .. } => "lettermint",
        ProviderConfig::Postmark { .. } => "postmark",
        ProviderConfig::Sendgrid { .. } => "sendgrid",
        ProviderConfig::Smtp { .. } => "smtp",
    }
}

/// True for the provider tags accepted from the integration form.
pub fn provider_is_known(name: &str) -> bool {
    matches!(name, "lettermint" | "postmark" | "sendgrid" | "smtp")
}

/// The API token/api_key held by an API-provider config (`None` for SMTP, whose
/// credential is the whole connection block rather than a single token). Empty
/// values read as absent so a blank placeholder in TOML never counts as a token.
pub fn api_credential(cfg: &ProviderConfig) -> Option<&str> {
    match cfg {
        ProviderConfig::Lettermint { token } | ProviderConfig::Postmark { token } => {
            Some(token.as_str())
        }
        ProviderConfig::Sendgrid { api_key } => Some(api_key.as_str()),
        ProviderConfig::Smtp { .. } => None,
    }
}

/// The instance-wide API token usable as a fallback for `provider`: only the
/// configured provider's own token applies (a Postmark token can't send via
/// SendGrid). Blank reads as absent.
pub fn global_api_token<'a>(global: &'a ProviderConfig, provider: &str) -> Option<&'a str> {
    (provider_label(global) == provider)
        .then(|| api_credential(global))
        .flatten()
        .filter(|t| !t.trim().is_empty())
}

fn api_provider_config(provider: &str, token: String) -> ProviderConfig {
    match provider {
        "lettermint" => ProviderConfig::Lettermint { token },
        "postmark" => ProviderConfig::Postmark { token },
        "sendgrid" => ProviderConfig::Sendgrid { api_key: token },
        _ => unreachable!("caller guarantees an API provider"),
    }
}

/// Resolves the `ProviderConfig` to send with, honoring `lock` and the
/// per-integration override. Locked installs use the instance provider verbatim;
/// otherwise the integration picks the provider tag and supplies its own token
/// (falling back to the matching instance token). SMTP carries no per-integration
/// credential, so it's only reachable when the instance provider is itself SMTP.
fn resolve_provider(
    email_cfg: &EmailConfig,
    int_provider: Option<&str>,
    secret: Option<&str>,
) -> Result<ProviderConfig> {
    if email_cfg.lock {
        return Ok(email_cfg.provider.clone());
    }
    // Absent provider means a row predating provider selection -- those are Postmark.
    let provider = int_provider.unwrap_or("postmark");
    match provider {
        "smtp" => match &email_cfg.provider {
            cfg @ ProviderConfig::Smtp { .. } => Ok(cfg.clone()),
            _ => anyhow::bail!(
                "email integration selects smtp but the instance [email] provider is not smtp"
            ),
        },
        "lettermint" | "postmark" | "sendgrid" => {
            let token = secret
                .map(str::to_string)
                .or_else(|| global_api_token(&email_cfg.provider, provider).map(str::to_string))
                .ok_or_else(|| anyhow::anyhow!("email provider token not configured"))?;
            Ok(api_provider_config(provider, token))
        }
        other => anyhow::bail!("unknown email provider '{other}'"),
    }
}

pub async fn send(
    email_cfg: &EmailConfig,
    base_url: &str,
    secret: Option<&str>,
    integration_config: Option<&str>,
    project_config: Option<&str>,
    event: &NotificationEvent,
) -> Result<()> {
    if !email_cfg.enabled {
        anyhow::bail!("email sending is disabled (email.enabled = false)");
    }

    let int_cfg =
        integration_config.and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok());
    let int_str = |key: &str| {
        int_cfg
            .as_ref()
            .and_then(|v| v.get(key).and_then(|f| f.as_str()).map(String::from))
    };

    let (from, name) = if email_cfg.lock {
        (email_cfg.from_address.clone(), email_cfg.from_name.clone())
    } else {
        (
            int_str("from").or_else(|| email_cfg.from_address.clone()),
            int_str("from_name").or_else(|| email_cfg.from_name.clone()),
        )
    };
    let provider_cfg = resolve_provider(email_cfg, int_str("provider").as_deref(), secret)?;

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

    let label = provider_label(&provider_cfg);
    provider_cfg
        .build()
        .map_err(|e| anyhow::anyhow!("{label} mailer build failed: {e}"))?
        .send(&email)
        .await
        .map_err(|e| anyhow::anyhow!("{label} send failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use polymail::provider::smtp::SmtpTls;

    fn smtp_provider(tls: SmtpTls, user: Option<&str>) -> ProviderConfig {
        ProviderConfig::Smtp {
            host: "localhost".into(),
            port: Some(1025),
            tls,
            user: user.map(String::from),
            pass: user.map(|_| "pass".into()),
        }
    }

    #[test]
    fn provider_label_matches_tag() {
        assert_eq!(
            provider_label(&ProviderConfig::Postmark { token: "t".into() }),
            "postmark"
        );
        assert_eq!(provider_label(&smtp_provider(SmtpTls::None, None)), "smtp");
        assert!(provider_is_known("sendgrid"));
        assert!(!provider_is_known("mailgun"));
    }

    #[test]
    fn global_token_only_matches_same_provider() {
        let global = ProviderConfig::Postmark {
            token: "pm-token".into(),
        };
        assert_eq!(global_api_token(&global, "postmark"), Some("pm-token"));
        // A Postmark token is not a fallback for a SendGrid integration.
        assert_eq!(global_api_token(&global, "sendgrid"), None);
        // Blank reads as absent.
        let blank = ProviderConfig::Postmark { token: "".into() };
        assert_eq!(global_api_token(&blank, "postmark"), None);
    }

    #[test]
    fn locked_ignores_integration_provider() {
        let cfg = EmailConfig {
            enabled: true,
            from_address: Some("a@b.c".into()),
            from_name: None,
            lock: true,
            provider: ProviderConfig::Sendgrid {
                api_key: "sg".into(),
            },
        };
        // Even a stored "postmark" is ignored when locked.
        let resolved = resolve_provider(&cfg, Some("postmark"), None).unwrap();
        assert_eq!(provider_label(&resolved), "sendgrid");
    }

    #[test]
    fn unlocked_prefers_integration_secret_over_global() {
        let cfg = EmailConfig {
            enabled: true,
            from_address: None,
            from_name: None,
            lock: false,
            provider: ProviderConfig::Postmark {
                token: "global".into(),
            },
        };
        let resolved = resolve_provider(&cfg, Some("postmark"), Some("per-int")).unwrap();
        match resolved {
            ProviderConfig::Postmark { token } => assert_eq!(token, "per-int"),
            other => panic!("expected postmark, got {}", provider_label(&other)),
        }
    }

    #[test]
    fn unlocked_smtp_requires_instance_smtp() {
        let api = EmailConfig {
            enabled: true,
            from_address: None,
            from_name: None,
            lock: false,
            provider: ProviderConfig::Postmark { token: "t".into() },
        };
        assert!(resolve_provider(&api, Some("smtp"), None).is_err());

        let smtp = EmailConfig {
            enabled: true,
            from_address: None,
            from_name: None,
            lock: false,
            provider: smtp_provider(SmtpTls::None, None),
        };
        assert_eq!(
            provider_label(&resolve_provider(&smtp, Some("smtp"), None).unwrap()),
            "smtp"
        );
    }

    // Building the SMTP transport constructs a lettre connection pool, which
    // needs a Tokio runtime in scope (as it does on the real dispatch path).
    #[tokio::test]
    async fn smtp_plaintext_sink_builds() {
        assert!(smtp_provider(SmtpTls::None, None).build().is_ok());
    }

    #[test]
    fn smtp_credentials_over_plaintext_rejected_by_polymail() {
        // No runtime needed: polymail rejects creds-over-plaintext before it
        // touches the async transport.
        assert!(smtp_provider(SmtpTls::None, Some("user")).build().is_err());
    }

    // Real delivery against a local plaintext sink (mailcrab/MailHog on 1025),
    // driving the actual send() path end to end. Ignored by default; run with
    // `cargo test -- --ignored smtp_send_delivers` while a sink is listening.
    #[tokio::test]
    #[ignore = "requires an SMTP sink on localhost:1025"]
    async fn smtp_send_delivers_to_local_sink() {
        let email_cfg = EmailConfig {
            enabled: true,
            from_address: Some("alerts@stackpit.test".into()),
            from_name: Some("Stackpit".into()),
            lock: true,
            provider: smtp_provider(SmtpTls::None, None),
        };
        let event = NotificationEvent {
            trigger: crate::notify::NotifyTrigger::NewIssue,
            project_id: 42,
            fingerprint: "smtp-live-fingerprint".into(),
            title: Some("SMTP live delivery check".into()),
            level: Some("error".into()),
            environment: Some("test".into()),
            environments: vec!["test".into()],
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
