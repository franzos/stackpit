use crate::db::DbPool;
use crate::queries;
use crate::queries::releases::ReleaseUpsert;
use crate::sync::client::{SentryClient, SentryProject};
use anyhow::Result;

pub async fn sync_releases(
    pool: &DbPool,
    client: &SentryClient,
    org: &str,
    project: &SentryProject,
) -> Result<()> {
    tracing::info!(project = %project.slug, "syncing releases");

    let mut cursor: Option<String> = None;
    let mut synced: u64 = 0;

    loop {
        let (releases, next_cursor, has_next) = client
            .list_releases(org, Some(project.id), cursor.as_deref())
            .await?;

        if releases.is_empty() {
            break;
        }

        for release in &releases {
            let commit_sha = release.last_commit.as_ref().and_then(|c| c.id.as_deref());

            let info = ReleaseUpsert {
                version: &release.version,
                commit_sha,
                date_released: release.date_released.as_deref().and_then(parse_iso_ts),
                first_event: release.first_event.as_deref().and_then(parse_iso_ts),
                last_event: release.last_event.as_deref().and_then(parse_iso_ts),
                new_groups: release.new_groups.unwrap_or(0),
            };

            queries::releases::upsert_release(pool, project.id, &info).await?;

            synced += 1;
        }

        if !has_next {
            break;
        }

        cursor = next_cursor;
    }

    tracing::info!(project = %project.slug, count = synced, "releases synced");
    Ok(())
}

fn parse_iso_ts(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp())
}
