use crate::queries;
use anyhow::Result;

pub async fn run(pool: &crate::db::DbPool) -> Result<()> {
    let projects = queries::projects::list_projects(pool, None, None, None).await?;

    if projects.is_empty() {
        println!("No projects found.");
        return Ok(());
    }

    println!(
        "{:<12} {:>8} {:>20} {:>20}",
        "PROJECT ID", "EVENTS", "FIRST SEEN", "LAST SEEN"
    );
    println!("{}", "-".repeat(64));

    for p in projects {
        let first = chrono::DateTime::from_timestamp(p.first_seen, 0)
            .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "?".to_string());
        let last = chrono::DateTime::from_timestamp(p.last_seen, 0)
            .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "?".to_string());

        println!(
            "{:<12} {:>8} {:>20} {:>20}",
            p.project_id, p.event_count, first, last
        );
    }

    Ok(())
}
