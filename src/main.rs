use clap::{Parser, Subcommand};
use std::future::Future;
use std::path::PathBuf;

use stackpit::{cli, config, db, server, sync};

/// Build a multi-thread runtime and drive `fut` to completion.
fn cli_run<F, T>(fut: F) -> anyhow::Result<T>
where
    F: Future<Output = anyhow::Result<T>>,
{
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(fut)
}

#[derive(Parser)]
#[command(name = "stackpit", about = "Drop-in Sentry host replacement")]
struct Cli {
    /// Config file path
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fire up the HTTP server
    Serve {
        /// Run ingest-only (no admin UI or API)
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

        /// Cap pages per project (handy for testing)
        #[arg(long)]
        max_pages: Option<u32>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from(config::DEFAULT_CONFIG_PATH));

    if let Command::Init = cli.command {
        return cli::init::run(&config_path);
    }

    // An explicit --config that doesn't exist is a hard error; a missing default
    // path falls back to built-in defaults so a fresh checkout still boots.
    let config = config::Config::load(&config_path, cli.config.is_some())?;
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

            cli_run(server::run(config, ingest_only))?;
        }
        Command::Projects => {
            cli_run(async {
                let pool = db::create_pool(&database_url).await?;
                cli::projects::run(&pool).await
            })?;
        }
        Command::Events { project, limit } => {
            cli_run(async {
                let pool = db::create_pool(&database_url).await?;
                cli::events::run(&pool, project, limit).await
            })?;
        }
        Command::Event { id } => {
            cli_run(async {
                let pool = db::create_pool(&database_url).await?;
                cli::event::run(&pool, &id).await
            })?;
        }
        Command::Tail => {
            cli_run(async {
                let pool = db::create_pool(&database_url).await?;
                cli::tail::run(&pool).await
            })?;
        }
        Command::Status => {
            cli_run(cli::status::run(&config))?;
        }
        Command::BackfillIssues => {
            cli_run(async {
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
