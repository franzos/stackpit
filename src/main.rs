mod api;
mod auth;
mod auth_service;
mod background;
mod cli;
mod config;
mod crypto;
mod db;
mod encoding;
mod endpoints;
mod enrich;
mod envelope;
mod event_data;
mod extractors;
mod filter;
mod fingerprint;
mod forge;
mod html;
mod middleware;
mod models;
mod network;
mod notify;
mod providers;
mod queries;
mod server;
mod sourcemap;
mod ssrf;
mod stats;
mod sync;
mod writer;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "stackpit", about = "Drop-in Sentry host replacement")]
struct Cli {
    /// Config file path
    #[arg(short, long, default_value = "stackpit.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fire up the HTTP server
    Serve {
        /// Run ingest-only -- no admin UI or API
        #[arg(long)]
        ingest_only: bool,
    },

    /// Show known projects
    Projects,

    /// Show recent events
    Events {
        /// Only show events for this project
        #[arg(short, long)]
        project: Option<u64>,

        /// How many to show
        #[arg(short, long, default_value = "20")]
        limit: u32,
    },

    /// Dump a single event as decompressed JSON
    Event { id: String },

    /// Follow new events as they come in
    Tail,

    /// Write a default config file
    Init,

    /// Show environment and configuration overview
    Status,

    /// Retroactively generate fingerprints and issues for old events
    BackfillIssues,

    /// Pull events from a remote Sentry instance
    Sync {
        /// Sentry org slug
        #[arg(long)]
        org: String,

        /// API base URL
        #[arg(long, default_value = "https://sentry.io")]
        url: String,

        /// Limit to these projects (comma-separated slugs)
        #[arg(long, value_delimiter = ',')]
        projects: Option<Vec<String>>,

        /// Cap pages per project -- handy for testing
        #[arg(long)]
        max_pages: Option<u32>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Command::Init = cli.command {
        return cli::init::run(&cli.config);
    }

    let config = config::Config::load(&cli.config)?;
    config.validate()?;

    let database_url = config.storage.database_url();

    match cli.command {
        Command::Serve { ingest_only } => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "stackpit=info,tower_http=info".into()),
                )
                .init();

            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(server::run(config, ingest_only))?;
        }
        Command::Projects => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(async {
                let pool = db::create_pool(&database_url).await?;
                cli::projects::run(&pool).await
            })?;
        }
        Command::Events { project, limit } => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(async {
                let pool = db::create_pool(&database_url).await?;
                cli::events::run(&pool, project, limit).await
            })?;
        }
        Command::Event { id } => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(async {
                let pool = db::create_pool(&database_url).await?;
                cli::event::run(&pool, &id).await
            })?;
        }
        Command::Tail => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(async {
                let pool = db::create_pool(&database_url).await?;
                cli::tail::run(&pool).await
            })?;
        }
        Command::Status => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(cli::status::run(&config))?;
        }
        Command::BackfillIssues => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(async {
                let pool = db::create_writer_pool(&database_url).await?;
                db::run_migrations(&pool).await?;
                cli::backfill::run(&pool).await
            })?;
        }
        Command::Sync {
            org,
            url,
            projects,
            max_pages,
        } => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "stackpit=info".into()),
                )
                .init();

            sync::run(
                &database_url,
                sync::SyncArgs {
                    org,
                    url,
                    projects,
                    max_pages,
                },
            )?;
        }
        Command::Init => unreachable!(),
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Integration tests: event ingestion → writer → query → response
// ---------------------------------------------------------------------------

#[cfg(test)]
mod integration_tests {
    use crate::db;
    use crate::models::{ItemType, StorableEvent};
    use crate::writer::{self, WriteMsg};

    fn make_event(event_id: &str, project_id: u64, fingerprint: &str) -> StorableEvent {
        let payload = serde_json::json!({
            "event_id": event_id,
            "message": format!("test error {event_id}"),
            "level": "error",
            "timestamp": 1700000000.0,
        });
        let raw = serde_json::to_vec(&payload).unwrap();

        StorableEvent {
            event_id: event_id.to_string(),
            item_type: ItemType::Event,
            payload: raw,
            project_id,
            public_key: "test-key".to_string(),
            timestamp: 1700000000,
            level: Some("error".to_string()),
            platform: Some("python".to_string()),
            release: Some("1.0.0".to_string()),
            environment: Some("production".to_string()),
            server_name: None,
            transaction_name: None,
            title: Some(format!("test error {event_id}")),
            sdk_name: None,
            sdk_version: None,
            fingerprint: Some(fingerprint.to_string()),
            monitor_slug: None,
            session_status: None,
            parent_event_id: None,
            user_identifier: Some("user-1".to_string()),
            tags: vec![("browser".to_string(), "Chrome".to_string())],
        }
    }

    #[tokio::test]
    async fn ingest_event_then_query_back() {
        let pool = db::open_test_pool().await;

        // Need a project row in the DB for queries to work
        sqlx::query("INSERT INTO projects (project_id, name) VALUES (1, 'test-project')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO project_keys (public_key, project_id, status) VALUES ('test-key', 1, 'active')")
            .execute(&pool).await.unwrap();

        // Spin up the writer
        let (writer, _join) = writer::spawn(
            pool.clone(),
            None,
            None,
            std::sync::Arc::new(crate::stats::IngestStats::new()),
        )
        .await
        .unwrap();
        let tx = writer.raw_sender();

        // Push events through the raw channel
        let event1 = make_event("evt-001", 1, "fp-001");
        let event2 = make_event("evt-002", 1, "fp-001"); // same fingerprint, should land in same issue

        tx.try_send(WriteMsg::Event(event1)).unwrap();
        tx.try_send(WriteMsg::Event(event2)).unwrap();

        // Shut down cleanly so everything gets flushed
        let _ = writer.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Event should be queryable
        let detail = crate::queries::events::get_event_detail(&pool, "evt-001")
            .await
            .unwrap();
        assert!(
            detail.is_some(),
            "event should be queryable after ingestion"
        );
        let detail = detail.unwrap();
        assert_eq!(detail.event_id, "evt-001");

        // Issue should've been created from the fingerprint
        let issue = crate::queries::issues::get_issue(&pool, "fp-001")
            .await
            .unwrap();
        assert!(issue.is_some(), "issue should exist for the fingerprint");
        let issue = issue.unwrap();
        assert_eq!(issue.fingerprint, "fp-001");
        assert!(
            issue.event_count >= 2,
            "issue should have at least 2 events"
        );

        // Both events should show up in the project listing
        let page = crate::queries::types::Page::new(None, None);
        let events = crate::queries::events::list_events(&pool, 1, &page)
            .await
            .unwrap();
        assert!(
            events.items.len() >= 2,
            "project should have at least 2 events"
        );
    }

    #[tokio::test]
    async fn ingest_and_update_issue_status() {
        let pool = db::open_test_pool().await;

        sqlx::query("INSERT INTO projects (project_id, name) VALUES (1, 'test-project')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO project_keys (public_key, project_id, status) VALUES ('test-key', 1, 'active')")
            .execute(&pool).await.unwrap();

        let (writer, _join) = writer::spawn(
            pool.clone(),
            None,
            None,
            std::sync::Arc::new(crate::stats::IngestStats::new()),
        )
        .await
        .unwrap();

        writer
            .send_event(make_event("evt-status-1", 1, "fp-status-001"))
            .unwrap();

        // Wait past the 1s flush interval, then send another event to
        // trigger the aggregation flush that creates the issue row.
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        writer
            .send_event(make_event("evt-status-2", 1, "fp-status-002"))
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Issue should exist now -- try updating its status
        let reply_rx = writer
            .update_issue_status(
                "fp-status-001".to_string(),
                crate::queries::IssueStatus::Resolved,
            )
            .unwrap();

        let result = reply_rx.await;
        assert!(result.is_ok(), "should get a reply from writer");

        let _ = writer.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Confirm the status change stuck
        let issue = crate::queries::issues::get_issue(&pool, "fp-status-001")
            .await
            .unwrap();
        assert!(issue.is_some());
        let issue = issue.unwrap();
        assert_eq!(issue.status, crate::queries::IssueStatus::Resolved);
    }
}
