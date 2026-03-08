use crate::db::DbPool;
use crate::queries;
use crate::sync::client::{SentryClient, SentryProject};
use crate::sync::state;
use crate::sync::transform;
use anyhow::Result;

pub async fn sync_project_events(
    pool: &DbPool,
    client: &SentryClient,
    org: &str,
    project: &SentryProject,
    max_pages: Option<u32>,
) -> Result<()> {
    let state_key = format!("sync:{}:{}:last_timestamp", org, project.slug);
    let last_ts = state::get_checkpoint(pool, &state_key).await?;

    // Subtract 5 minutes from the checkpoint to catch any stragglers
    let start = last_ts.map(|ts| {
        chrono::DateTime::from_timestamp(ts.saturating_sub(300), 0)
            .unwrap_or_default()
            .to_rfc3339()
    });

    println!(
        "\n[{}] syncing events{}...",
        project.slug,
        start
            .as_ref()
            .map(|s| format!(" from {s}"))
            .unwrap_or_default()
    );

    let cursor_key = format!("sync:{}:{}:last_cursor", org, project.slug);
    let mut cursor: Option<String> = state::get_checkpoint_str(pool, &cursor_key).await?;
    if cursor.is_some() {
        println!("  resuming from saved cursor");
    }
    let mut pages: u32 = 0;
    let mut fetched: u64 = 0;
    let mut inserted: u64 = 0;
    let mut max_timestamp: Option<i64> = last_ts;

    loop {
        if let Some(max) = max_pages {
            if pages >= max {
                println!("  reached max pages ({max}), stopping");
                break;
            }
        }

        let page = client
            .list_events(org, &project.slug, start.as_deref(), cursor.as_deref())
            .await?;

        if page.events.is_empty() {
            break;
        }

        let page_count = page.events.len();
        fetched += page_count as u64;

        for sentry_event in &page.events {
            let storable = transform::to_storable_event(sentry_event, project.id)?;
            let is_new = queries::event_writes::insert_event_row(pool, &storable).await?;
            if is_new {
                queries::event_writes::upsert_issue_from_event(pool, &storable).await?;
                inserted += 1;
            }

            // Stash the Sentry groupID so we can reliably sync statuses later
            if let (Some(ref fp), Some(group_id)) = (
                &storable.fingerprint,
                sentry_event.json.get("groupID").and_then(|v| v.as_str()),
            ) {
                queries::issues::set_sentry_group_id(pool, fp, group_id).await?;
            }

            // Track the latest timestamp for our checkpoint
            if let Some(ts) = sentry_event.timestamp() {
                max_timestamp = Some(max_timestamp.map_or(ts, |prev: i64| prev.max(ts)));
            }
        }

        pages += 1;
        println!("  page {pages} | {fetched} fetched | {inserted} new");

        // Persist cursor so we can resume if something goes wrong mid-run
        if let Some(ref c) = page.next_cursor {
            state::set_checkpoint(pool, &cursor_key, c).await?;
        }

        if !page.has_next {
            break;
        }

        cursor = page.next_cursor;
    }

    // Save our high-water mark
    if let Some(ts) = max_timestamp {
        state::set_checkpoint(pool, &state_key, &ts.to_string()).await?;
    }

    // Done paginating -- clear the cursor
    state::clear_checkpoint(pool, &cursor_key).await?;

    println!(
        "[{}] events: {fetched} fetched, {inserted} new",
        project.slug
    );
    Ok(())
}
