use crate::db::DbPool;
use crate::queries;
use crate::sync::client::{SentryClient, SentryProject};
use anyhow::Result;

pub async fn sync_issue_statuses(
    pool: &DbPool,
    client: &SentryClient,
    org: &str,
    project: &SentryProject,
) -> Result<()> {
    println!("[{}] syncing issue statuses...", project.slug);

    let mut cursor: Option<String> = None;
    let mut synced: u64 = 0;
    let mut updated: u64 = 0;

    loop {
        let page = client
            .list_issues(org, project.id, cursor.as_deref())
            .await?;

        if page.issues.is_empty() {
            break;
        }

        for issue in &page.issues {
            let sentry_status = normalize_status(&issue.status);

            // We only care about resolved/ignored here -- new events already
            // handle the resolved -> unresolved transition on their own
            if sentry_status == "unresolved" {
                synced += 1;
                continue;
            }

            // Match by sentry_group_id first (reliable), fall back to title if needed
            let changed = queries::issues::update_status_by_group_id(
                pool,
                project.id,
                &issue.id,
                sentry_status,
            )
            .await?;

            updated += changed;
            synced += 1;
        }

        if !page.has_next {
            break;
        }

        cursor = page.next_cursor;
    }

    println!(
        "[{}] issues: {synced} checked, {updated} status updates",
        project.slug
    );
    Ok(())
}

fn normalize_status(sentry_status: &str) -> &str {
    match sentry_status {
        "resolved" => "resolved",
        "ignored" | "muted" => "ignored",
        _ => "unresolved",
    }
}
