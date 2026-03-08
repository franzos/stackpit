use crate::queries;
use anyhow::{bail, Result};

pub async fn run(pool: &crate::db::DbPool, event_id: &str) -> Result<()> {
    let detail = match queries::events::get_event_detail(pool, event_id).await? {
        Some(d) => d,
        None => bail!("event not found: {event_id}"),
    };

    let pretty = serde_json::to_string_pretty(&detail.payload)?;

    println!("{pretty}");
    Ok(())
}
