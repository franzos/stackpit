use crate::queries;
use anyhow::Result;

pub async fn run(pool: &crate::db::DbPool) -> Result<()> {
    let mut last_received_at = chrono::Utc::now().timestamp();

    println!("tailing events (Ctrl+C to stop)...");

    loop {
        let events = queries::events::tail_events(pool, last_received_at).await?;

        for e in &events {
            let ts = chrono::DateTime::from_timestamp(e.timestamp, 0)
                .map(|d| d.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| "?".to_string());

            let level = e.level.as_deref().unwrap_or("-");
            let title = e.title.as_deref().unwrap_or("-");

            println!(
                "[{ts}] project={} type={} level={level} {title}",
                e.project_id, e.item_type
            );

            if e.received_at > last_received_at {
                last_received_at = e.received_at;
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
