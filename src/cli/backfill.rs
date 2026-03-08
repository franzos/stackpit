use std::str::FromStr;

use anyhow::Result;

use crate::db::DbPool;
use crate::fingerprint;
use crate::models;
use crate::queries::backfill;
use crate::queries::BackfillRow;

pub async fn run(pool: &DbPool) -> Result<()> {
    let total = backfill::count_missing_fingerprints(pool).await?;

    if total == 0 {
        println!("no events need backfilling");
        return Ok(());
    }

    println!("backfilling {total} events...");

    let mut processed: u64 = 0;
    let mut skipped: u64 = 0;

    loop {
        let batch = backfill::fetch_events_without_fingerprint(pool, 1000).await?;

        if batch.is_empty() {
            break;
        }

        let (batch_processed, batch_skipped) = process_backfill_batch(pool, &batch).await?;
        processed += batch_processed;
        skipped += batch_skipped;

        println!("  processed {processed}/{total} (skipped {skipped})");
    }

    println!("backfill complete: {processed} fingerprinted, {skipped} skipped");
    Ok(())
}

/// Tries to compute a fingerprint for a backfill row. Returns `None` if the
/// item type isn't fingerprintable or the payload won't decode.
fn compute_row_fingerprint(row: &BackfillRow) -> Option<String> {
    let item_type = models::ItemType::from_str(&row.item_type_str).unwrap();

    match item_type {
        models::ItemType::Event | models::ItemType::Transaction => {}
        _ => return None,
    }

    let payload_json = zstd::decode_all(row.payload_blob.as_slice()).ok()?;

    fingerprint::compute_fingerprint(row.project_id, &item_type, &payload_json)
}

/// Runs through a batch of events that don't have fingerprints yet.
async fn process_backfill_batch(pool: &DbPool, batch: &[BackfillRow]) -> Result<(u64, u64)> {
    let mut processed: u64 = 0;
    let mut skipped: u64 = 0;

    for row in batch {
        let fp = match compute_row_fingerprint(row) {
            Some(fp) => fp,
            None => {
                skipped += 1;
                continue;
            }
        };

        backfill::set_event_fingerprint(pool, &row.event_id, &fp).await?;

        let item_type = models::ItemType::from_str(&row.item_type_str).unwrap();
        backfill::upsert_backfill_issue(
            pool,
            &fp,
            row.project_id,
            row.title.as_deref(),
            row.level.as_deref(),
            row.timestamp,
            &item_type,
        )
        .await?;

        processed += 1;
    }

    Ok((processed, skipped))
}
