use crate::queries;
use crate::queries::types::{EventFilter, Page};
use anyhow::Result;

pub async fn run(pool: &crate::db::DbPool, project_id: Option<u64>, limit: u32) -> Result<()> {
    let filter = EventFilter {
        project_id,
        ..Default::default()
    };
    let page = Page::new(Some(0), Some(limit as u64));
    let events = queries::events::list_all_events(pool, &filter, &page)
        .await?
        .items;

    if events.is_empty() {
        println!("No events found.");
        return Ok(());
    }

    println!(
        "{:<36} {:<12} {:>6} {:<8} TITLE",
        "EVENT ID", "PROJECT", "TYPE", "LEVEL"
    );
    println!("{}", "-".repeat(100));

    for e in events {
        let level = e.level.as_deref().unwrap_or("-");
        let title = e.title.as_deref().unwrap_or("-");
        let title_truncated: String = title.chars().take(60).collect();

        println!(
            "{:<36} {:<12} {:>6} {:<8} {}",
            e.event_id, e.project_id, e.item_type, level, title_truncated
        );
    }

    Ok(())
}
