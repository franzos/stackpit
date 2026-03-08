mod attachments;
mod client;
mod events;
mod issues;
mod releases;
mod state;
mod transform;

use crate::db;
use crate::queries;
use anyhow::{bail, Result};

pub struct SyncArgs {
    pub org: String,
    pub url: String,
    pub projects: Option<Vec<String>>,
    pub max_pages: Option<u32>,
}

pub fn run(database_url: &str, args: SyncArgs) -> Result<()> {
    let token = std::env::var("SENTRY_AUTH_TOKEN")
        .map_err(|_| anyhow::anyhow!("SENTRY_AUTH_TOKEN environment variable not set"))?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(run_async(database_url, args, &token))
}

async fn run_async(database_url: &str, args: SyncArgs, token: &str) -> Result<()> {
    let client = client::SentryClient::new(&args.url, token)?;
    let pool = db::create_writer_pool(database_url).await?;
    db::run_migrations(&pool).await?;

    // Find out what projects exist in this org
    println!("discovering projects for org '{}'...", args.org);
    let projects = client.list_projects(&args.org).await?;

    if projects.is_empty() {
        bail!("no projects found in org '{}'", args.org);
    }

    // Narrow down to the ones we care about, if specified
    let projects: Vec<_> = if let Some(ref filter) = args.projects {
        projects
            .into_iter()
            .filter(|p| filter.iter().any(|f| f == &p.slug))
            .collect()
    } else {
        projects
    };

    println!("found {} project(s)", projects.len());
    for p in &projects {
        println!(
            "  {} (id={}, platform={})",
            p.slug,
            p.id,
            p.platform.as_deref().unwrap_or("-")
        );
    }

    // Make sure we have the project metadata and keys stored locally
    sync_project_metadata(&pool, &projects, &args.org, &client).await?;

    for project in &projects {
        events::sync_project_events(&pool, &client, &args.org, project, args.max_pages).await?;
        attachments::sync_attachments(&pool, &client, &args.org, project).await?;
        issues::sync_issue_statuses(&pool, &client, &args.org, project).await?;
        releases::sync_releases(&pool, &client, &args.org, project).await?;
    }

    println!("\nsync complete");
    Ok(())
}

async fn sync_project_metadata(
    pool: &db::DbPool,
    projects: &[client::SentryProject],
    org_slug: &str,
    sentry_client: &client::SentryClient,
) -> Result<()> {
    let org_id = queries::projects::upsert_organization(pool, org_slug, None).await?;
    for project in projects {
        queries::projects::upsert_synced_project(pool, project.id, &project.name, org_id).await?;

        match sentry_client
            .list_project_keys(org_slug, &project.slug)
            .await
        {
            Ok(keys) => {
                let mut synced = 0u32;
                for key in &keys {
                    let label = key.label.as_deref().or(key.name.as_deref());
                    queries::projects::upsert_synced_key(
                        pool,
                        project.id,
                        &key.public_key,
                        label,
                        key.is_active,
                    )
                    .await?;
                    synced += 1;
                }
                if synced > 0 {
                    println!("  [{}] {} key(s) synced", project.slug, synced);
                }
            }
            Err(e) => {
                // Keys endpoint may fail if the token lacks project:read scope.
                // Not fatal — the user can add keys manually via the settings UI.
                println!("  [{}] warning: could not sync keys ({})", project.slug, e);
            }
        }
    }
    Ok(())
}
