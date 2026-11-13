use crate::db::DbPool;
use crate::ingest::models::StorableAttachment;
use crate::ingest::text::clean_text;
use crate::queries;
use crate::sync::client::{SentryClient, SentryProject};
use anyhow::Result;

pub async fn sync_attachments(
    pool: &DbPool,
    client: &SentryClient,
    org: &str,
    project: &SentryProject,
) -> Result<()> {
    let event_ids =
        queries::event_sync::list_synced_events_without_attachments(pool, project.id).await?;

    if event_ids.is_empty() {
        return Ok(());
    }

    println!(
        "[{}] checking attachments for {} event(s)...",
        project.slug,
        event_ids.len()
    );

    let existing_attachments =
        queries::event_sync::list_existing_attachment_keys(pool, project.id).await?;

    let mut total_attachments: u64 = 0;

    for event_id in &event_ids {
        let attachments = match client
            .list_event_attachments(org, &project.slug, event_id)
            .await
        {
            Ok(a) => a,
            Err(e) => {
                tracing::debug!("failed to list attachments for {event_id}: {e}");
                continue;
            }
        };

        if attachments.is_empty() {
            continue;
        }

        for att_info in &attachments {
            let filename = clean_text(&att_info.name);
            if existing_attachments.contains(&(event_id.clone(), filename.clone())) {
                continue;
            }

            let data = match client
                .download_attachment(org, &project.slug, event_id, &att_info.id)
                .await
            {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(
                        "failed to download attachment {} for {event_id}: {e}",
                        att_info.name
                    );
                    continue;
                }
            };

            let storable_att = StorableAttachment {
                event_id: event_id.clone(),
                filename,
                content_type: att_info.mimetype.as_deref().map(clean_text),
                data,
            };
            if queries::event_writes::insert_attachment(pool, project.id, &storable_att).await? {
                total_attachments += 1;
            }
        }
    }

    if total_attachments > 0 {
        println!(
            "[{}] attachments: {total_attachments} downloaded",
            project.slug
        );
    }

    Ok(())
}
