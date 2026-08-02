//! Sourcemap storage and resolution. Handles artifact bundle parsing
//! (ZIP files from `sentry-cli sourcemaps upload`) and on-the-fly
//! stack frame resolution using debug IDs.

use anyhow::{Context, Result};
use std::io::Read;

// Bundle parsing limits (ZIP directory is attacker-controlled)

const MAX_BUNDLE_ENTRIES: usize = 10_000;
const MAX_BUNDLE_ENTRY_BYTES: usize = 64 * 1024 * 1024; // 64 MiB per entry
pub const MAX_BUNDLE_TOTAL_BYTES: usize = 512 * 1024 * 1024; // 512 MiB total, zip-bomb guard

/// Distinct debug ids resolved for a single event. `debug_meta.images` is
/// attacker-controlled; a real bundled app stays well under this.
pub const MAX_EVENT_DEBUG_IDS: usize = 512;

const MIN_DEBUG_ID_LEN: usize = 8;
const MAX_DEBUG_ID_LEN: usize = 64;

/// Length sanity check only: debug id formats differ per platform.
pub fn is_plausible_debug_id(id: &str) -> bool {
    (MIN_DEBUG_ID_LEN..=MAX_DEBUG_ID_LEN).contains(&id.len())
}

/// Assembling persisted chunks exceeded the bundle size cap.
#[derive(Debug)]
pub struct BundleTooLarge {
    pub size: usize,
    pub limit: usize,
}

impl std::fmt::Display for BundleTooLarge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "assembled bundle exceeds size limit ({} > {} bytes)",
            self.size, self.limit
        )
    }
}

impl std::error::Error for BundleTooLarge {}

// Types

pub struct SourcemapEntry {
    pub debug_id: String,
    pub source_url: Option<String>,
    pub data: Vec<u8>,
}

pub struct ResolvedFrame {
    pub filename: String,
    pub function: Option<String>,
    pub lineno: u32,
    pub colno: u32,
    pub context_line: Option<String>,
    pub pre_context: Vec<String>,
    pub post_context: Vec<String>,
}

// Artifact bundle parsing

/// Parse sourcemap bundle (ZIP) offloaded to blocking task.
pub async fn parse_artifact_bundle(zip_data: Vec<u8>) -> Result<Vec<SourcemapEntry>> {
    tokio::task::spawn_blocking(move || parse_artifact_bundle_sync(&zip_data))
        .await
        .context("sourcemap bundle parse task join failed")?
}

fn parse_artifact_bundle_sync(zip_data: &[u8]) -> Result<Vec<SourcemapEntry>> {
    let cursor = std::io::Cursor::new(zip_data);
    let mut archive = zip::ZipArchive::new(cursor).context("invalid ZIP archive")?;

    if archive.len() > MAX_BUNDLE_ENTRIES {
        anyhow::bail!(
            "bundle exceeds entry count limit ({} > {MAX_BUNDLE_ENTRIES})",
            archive.len()
        );
    }

    // Manifest may be at the root or under artifact-bundle/.
    let manifest: serde_json::Value = try_read_manifest(&mut archive)?;

    let mut total_bytes: usize = 0;
    let mut entries = Vec::new();

    if let Some(files) = manifest.get("files").and_then(|f| f.as_object()) {
        for (zip_path, meta) in files {
            let file_type = meta.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if file_type != "source_map" && file_type != "sourcemap" {
                continue;
            }

            let debug_id = extract_debug_id(meta);
            let debug_id = match debug_id {
                Some(id) => id,
                None => continue,
            };

            let source_url = meta
                .get("url")
                .or_else(|| meta.get("abs_path"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let data = match read_zip_entry(&mut archive, zip_path) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!("skipping {zip_path}: {e}");
                    continue;
                }
            };

            total_bytes = total_bytes.saturating_add(data.len());
            if total_bytes > MAX_BUNDLE_TOTAL_BYTES {
                anyhow::bail!(
                    "bundle exceeds total decompressed size limit ({total_bytes} > {MAX_BUNDLE_TOTAL_BYTES})"
                );
            }

            entries.push(SourcemapEntry {
                debug_id,
                source_url,
                data,
            });
        }
    }

    // Fallback when no manifest entries: scan .map files for embedded debug_id.
    if entries.is_empty() {
        entries = scan_for_sourcemaps(&mut archive)?;
    }

    Ok(entries)
}

fn try_read_manifest(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
) -> Result<serde_json::Value> {
    for name in &["manifest.json", "artifact-bundle/manifest.json"] {
        if let Ok(data) = read_zip_entry(archive, name) {
            if let Ok(val) = serde_json::from_slice(&data) {
                return Ok(val);
            }
        }
    }
    Ok(serde_json::Value::Object(serde_json::Map::new()))
}

fn extract_debug_id(meta: &serde_json::Value) -> Option<String> {
    if let Some(id) = meta
        .get("headers")
        .and_then(|h| h.get("debug-id"))
        .and_then(|v| v.as_str())
    {
        return Some(normalize_debug_id(id));
    }
    if let Some(id) = meta
        .get("debug_id")
        .or_else(|| meta.get("debugId"))
        .and_then(|v| v.as_str())
    {
        return Some(normalize_debug_id(id));
    }
    None
}

/// Normalize debug IDs; strips the optional `-sourcemap` suffix.
fn normalize_debug_id(id: &str) -> String {
    id.split_once("-sourcemap")
        .map(|(base, _)| base.to_string())
        .unwrap_or_else(|| id.to_string())
        .to_lowercase()
}

fn read_zip_entry(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    name: &str,
) -> Result<Vec<u8>> {
    let file = archive.by_name(name)?;
    // `file.size()` is attacker-controlled; `Read::take` bounds the decompressed read against zip bombs.
    let cap = file.size().min(MAX_BUNDLE_ENTRY_BYTES as u64) as usize;
    let mut buf = Vec::with_capacity(cap);
    let limit = MAX_BUNDLE_ENTRY_BYTES as u64 + 1;
    let mut bounded = file.take(limit);
    bounded.read_to_end(&mut buf)?;
    if buf.len() > MAX_BUNDLE_ENTRY_BYTES {
        anyhow::bail!(
            "bundle entry {name} exceeds size limit ({} > {MAX_BUNDLE_ENTRY_BYTES})",
            buf.len()
        );
    }
    Ok(buf)
}

/// Fallback: scan all .map files in the archive for embedded debug_id
fn scan_for_sourcemaps(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
) -> Result<Vec<SourcemapEntry>> {
    let mut total_bytes: usize = 0;
    let mut entries = Vec::new();
    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();

    for name in &names {
        if !name.ends_with(".map") {
            continue;
        }
        let data = match read_zip_entry(archive, name) {
            Ok(d) => d,
            Err(_) => continue,
        };

        total_bytes = total_bytes.saturating_add(data.len());
        if total_bytes > MAX_BUNDLE_TOTAL_BYTES {
            anyhow::bail!(
                "bundle exceeds total decompressed size limit ({total_bytes} > {MAX_BUNDLE_TOTAL_BYTES})"
            );
        }

        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&data) {
            let debug_id = val
                .get("debug_id")
                .or_else(|| val.get("debugId"))
                .and_then(|v| v.as_str())
                .map(normalize_debug_id);

            if let Some(id) = debug_id {
                entries.push(SourcemapEntry {
                    debug_id: id,
                    source_url: Some(name.clone()),
                    data,
                });
            }
        }
    }

    Ok(entries)
}

// Frame resolution

/// Context lines to show above/below the error line
const CONTEXT_LINES: usize = 5;

/// Resolve a minified stack frame using a parsed sourcemap.
pub fn resolve_frame(sm: &sourcemap::SourceMap, line: u32, col: u32) -> Option<ResolvedFrame> {
    // sourcemap crate uses 0-indexed line/col
    let token = sm.lookup_token(line.saturating_sub(1), col.saturating_sub(1))?;

    let src_id = token.get_src_id();
    let orig_line = token.get_src_line(); // 0-indexed
    let orig_col = token.get_src_col();

    let filename = token.get_source().unwrap_or("<unknown>").to_string();
    let function = token.get_name().map(|s| s.to_string());

    let (context_line, pre_context, post_context) =
        if let Some(source) = sm.get_source_contents(src_id) {
            extract_context(source, orig_line as usize)
        } else {
            (None, Vec::new(), Vec::new())
        };

    Some(ResolvedFrame {
        filename,
        function,
        lineno: orig_line + 1, // back to 1-indexed
        colno: orig_col + 1,
        context_line,
        pre_context,
        post_context,
    })
}

fn extract_context(source: &str, line_idx: usize) -> (Option<String>, Vec<String>, Vec<String>) {
    let lines: Vec<&str> = source.lines().collect();

    if line_idx >= lines.len() {
        return (None, Vec::new(), Vec::new());
    }

    let context_line = Some(lines[line_idx].to_string());

    let pre_start = line_idx.saturating_sub(CONTEXT_LINES);
    let pre_context: Vec<String> = lines[pre_start..line_idx]
        .iter()
        .map(|s| s.to_string())
        .collect();

    let post_end = (line_idx + 1 + CONTEXT_LINES).min(lines.len());
    let post_context: Vec<String> = lines[line_idx + 1..post_end]
        .iter()
        .map(|s| s.to_string())
        .collect();

    (context_line, pre_context, post_context)
}

// DB helpers

use crate::db::{sql, DbPool};
use sqlx::Row;

/// Store a sourcemap entry (zstd-compressed) in the database.
pub async fn store_sourcemap(pool: &DbPool, entry: &SourcemapEntry, project_id: u64) -> Result<()> {
    let compressed =
        zstd::encode_all(entry.data.as_slice(), 3).context("zstd compress sourcemap")?;

    sqlx::query(sql!(
        "INSERT INTO sourcemaps (debug_id, source_url, data, project_id)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (project_id, debug_id) DO UPDATE SET data = ?3, source_url = ?2"
    ))
    .bind(&entry.debug_id)
    .bind(entry.source_url.as_deref())
    .bind(&compressed)
    .bind(project_id as i64)
    .execute(pool)
    .await?;

    Ok(())
}

/// Store a chunk for later assembly.
pub async fn store_chunk(
    pool: &DbPool,
    checksum: &str,
    data: &[u8],
    project_id: u64,
) -> Result<()> {
    sqlx::query(sql!(
        "INSERT INTO upload_chunks (checksum, project_id, data) VALUES (?1, ?2, ?3)
         ON CONFLICT (checksum, project_id) DO NOTHING"
    ))
    .bind(checksum)
    .bind(project_id as i64)
    .bind(data)
    .execute(pool)
    .await?;

    Ok(())
}

/// Return the subset of `checksums` that are not yet stored.
pub async fn find_missing_chunks(
    pool: &DbPool,
    checksums: &[String],
    project_id: u64,
) -> Result<Vec<String>> {
    if checksums.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders: Vec<String> = (1..=checksums.len()).map(|i| format!("?{i}")).collect();
    let pid_idx = checksums.len() + 1;
    let query = format!(
        "SELECT checksum FROM upload_chunks WHERE project_id = ?{pid_idx} AND checksum IN ({})",
        placeholders.join(", ")
    );
    let mut q = sqlx::query_scalar::<_, String>(crate::db::dyn_sql(&query));
    for cs in checksums {
        q = q.bind(cs.clone());
    }
    q = q.bind(project_id as i64);

    let found: Vec<String> = q.fetch_all(pool).await?;
    let found_set: std::collections::HashSet<&str> = found.iter().map(|s| s.as_str()).collect();
    Ok(checksums
        .iter()
        .filter(|cs| !found_set.contains(cs.as_str()))
        .cloned()
        .collect())
}

/// Read chunks in order and concatenate them into a single buffer,
/// capped at [`MAX_BUNDLE_TOTAL_BYTES`].
pub async fn assemble_chunks(
    pool: &DbPool,
    checksums: &[String],
    project_id: u64,
) -> Result<Vec<u8>> {
    assemble_chunks_capped(pool, checksums, project_id, MAX_BUNDLE_TOTAL_BYTES).await
}

// Per-request upload caps don't bound the total of persisted chunks, so the
// cumulative size must be checked here before the buffer grows.
async fn assemble_chunks_capped(
    pool: &DbPool,
    checksums: &[String],
    project_id: u64,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    for checksum in checksums {
        let row = sqlx::query(sql!(
            "SELECT data FROM upload_chunks WHERE checksum = ?1 AND project_id = ?2"
        ))
        .bind(checksum)
        .bind(project_id as i64)
        .fetch_optional(pool)
        .await?;

        match row {
            Some(row) => {
                let data: Vec<u8> = row.get("data");
                let size = result.len().saturating_add(data.len());
                if size > max_bytes {
                    return Err(BundleTooLarge {
                        size,
                        limit: max_bytes,
                    }
                    .into());
                }
                result.extend_from_slice(&data);
            }
            None => anyhow::bail!("missing chunk: {checksum}"),
        }
    }
    Ok(result)
}

/// Delete chunks after successful assembly.
pub async fn delete_chunks(pool: &DbPool, checksums: &[String], project_id: u64) -> Result<()> {
    for checksum in checksums {
        sqlx::query(sql!(
            "DELETE FROM upload_chunks WHERE checksum = ?1 AND project_id = ?2"
        ))
        .bind(checksum)
        .bind(project_id as i64)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Load and decompress a sourcemap by debug_id (scoped to project_id), then parse it.
pub async fn load_sourcemap(
    pool: &DbPool,
    debug_id: &str,
    project_id: u64,
) -> Result<Option<sourcemap::SourceMap>> {
    let row = sqlx::query(sql!(
        "SELECT data FROM sourcemaps WHERE debug_id = ?1 AND project_id = ?2"
    ))
    .bind(debug_id)
    .bind(project_id as i64)
    .fetch_optional(pool)
    .await?;

    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    let compressed: Vec<u8> = row.get("data");
    let sm = tokio::task::spawn_blocking(move || decode_sourcemap(&compressed))
        .await
        .context("sourcemap decode task join failed")??;

    Ok(Some(sm))
}

/// Load and parse several sourcemaps in one query (scoped to `project_id`).
/// Missing or unparseable ids are simply absent from the returned map.
pub async fn load_sourcemaps(
    pool: &DbPool,
    debug_ids: &[String],
    project_id: u64,
) -> Result<std::collections::HashMap<String, sourcemap::SourceMap>> {
    if debug_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let placeholders: Vec<String> = (1..=debug_ids.len()).map(|i| format!("?{i}")).collect();
    let pid_idx = debug_ids.len() + 1;
    let query = format!(
        "SELECT debug_id, data FROM sourcemaps WHERE project_id = ?{pid_idx} AND debug_id IN ({})",
        placeholders.join(", ")
    );
    let mut q = sqlx::query(crate::db::dyn_sql(&query));
    for id in debug_ids {
        q = q.bind(id.clone());
    }
    q = q.bind(project_id as i64);

    let rows = q.fetch_all(pool).await?;
    let compressed: Vec<(String, Vec<u8>)> = rows
        .iter()
        .map(|r| (r.get("debug_id"), r.get("data")))
        .collect();

    tokio::task::spawn_blocking(move || {
        compressed
            .into_iter()
            .filter_map(|(debug_id, data)| match decode_sourcemap(&data) {
                Ok(sm) => Some((debug_id, sm)),
                Err(e) => {
                    tracing::warn!("failed to parse sourcemap {debug_id}: {e}");
                    None
                }
            })
            .collect()
    })
    .await
    .context("sourcemap decode task join failed")
}

fn decode_sourcemap(compressed: &[u8]) -> Result<sourcemap::SourceMap> {
    let raw = zstd::decode_all(compressed).context("zstd decompress sourcemap")?;
    sourcemap::SourceMap::from_slice(&raw).context("parse sourcemap")
}

/// Delete sourcemaps older than `max_age_secs`. Tied to the same retention
/// window as events so old debug artifacts don't accumulate forever.
pub async fn cleanup_old_sourcemaps(pool: &DbPool, max_age_secs: i64) -> Result<u64> {
    let cutoff = chrono::Utc::now().timestamp() - max_age_secs;
    let result = sqlx::query(sql!("DELETE FROM sourcemaps WHERE created_at < ?1"))
        .bind(cutoff)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

/// Delete old upload chunks (stale uploads that were never assembled).
pub async fn cleanup_stale_chunks(pool: &DbPool, max_age_secs: i64) -> Result<u64> {
    let cutoff = chrono::Utc::now().timestamp() - max_age_secs;
    let result = sqlx::query(sql!("DELETE FROM upload_chunks WHERE created_at < ?1"))
        .bind(cutoff)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_test_pool;

    fn map_with_source(source: &str) -> Vec<u8> {
        format!(
            r#"{{"version":3,"sources":["{source}"],"names":[],"mappings":"AAAA","sourcesContent":["x"]}}"#
        )
        .into_bytes()
    }

    fn entry(debug_id: &str, source: &str) -> SourcemapEntry {
        SourcemapEntry {
            debug_id: debug_id.to_string(),
            source_url: None,
            data: map_with_source(source),
        }
    }

    #[tokio::test]
    async fn assemble_chunks_within_cap_concatenates() {
        let pool = open_test_pool().await;
        store_chunk(&pool, "aaaa", b"hello", 1).await.unwrap();
        store_chunk(&pool, "bbbb", b"world", 1).await.unwrap();

        let checksums = vec!["aaaa".to_string(), "bbbb".to_string()];
        let data = assemble_chunks_capped(&pool, &checksums, 1, 10)
            .await
            .unwrap();
        assert_eq!(data, b"helloworld");
    }

    #[tokio::test]
    async fn assemble_chunks_rejects_total_over_cap() {
        let pool = open_test_pool().await;
        store_chunk(&pool, "aaaa", b"hello", 1).await.unwrap();
        store_chunk(&pool, "bbbb", b"world", 1).await.unwrap();

        let checksums = vec!["aaaa".to_string(), "bbbb".to_string()];
        let err = assemble_chunks_capped(&pool, &checksums, 1, 9)
            .await
            .unwrap_err();
        assert!(err.downcast_ref::<BundleTooLarge>().is_some());
    }

    #[tokio::test]
    async fn assemble_chunks_counts_repeated_checksums() {
        let pool = open_test_pool().await;
        store_chunk(&pool, "aaaa", b"hello", 1).await.unwrap();

        let checksums = vec!["aaaa".to_string(); 3];
        let err = assemble_chunks_capped(&pool, &checksums, 1, 12)
            .await
            .unwrap_err();
        assert!(err.downcast_ref::<BundleTooLarge>().is_some());
    }

    #[tokio::test]
    async fn load_is_scoped_by_project() {
        let pool = open_test_pool().await;
        store_sourcemap(&pool, &entry("dead", "a.js"), 1)
            .await
            .unwrap();

        assert!(load_sourcemap(&pool, "dead", 1).await.unwrap().is_some());
        assert!(load_sourcemap(&pool, "dead", 2).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn batch_load_returns_found_ids_scoped_by_project() {
        let pool = open_test_pool().await;
        store_sourcemap(&pool, &entry("aaaaaaaa1111", "a.js"), 1)
            .await
            .unwrap();
        store_sourcemap(&pool, &entry("bbbbbbbb2222", "b.js"), 1)
            .await
            .unwrap();
        store_sourcemap(&pool, &entry("cccccccc3333", "c.js"), 2)
            .await
            .unwrap();

        let ids = vec![
            "aaaaaaaa1111".to_string(),
            "bbbbbbbb2222".to_string(),
            "cccccccc3333".to_string(),
            "missing00000".to_string(),
        ];
        let map = load_sourcemaps(&pool, &ids, 1).await.unwrap();

        assert_eq!(map.len(), 2);
        assert_eq!(map["aaaaaaaa1111"].get_source(0), Some("a.js"));
        assert_eq!(map["bbbbbbbb2222"].get_source(0), Some("b.js"));
    }

    #[tokio::test]
    async fn batch_load_empty_ids_skips_query() {
        let pool = open_test_pool().await;
        assert!(load_sourcemaps(&pool, &[], 1).await.unwrap().is_empty());
    }

    #[test]
    fn debug_id_shape_check_rejects_absurd_lengths() {
        assert!(is_plausible_debug_id("f3a1b2c4-1111-2222-3333-4444"));
        assert!(!is_plausible_debug_id("abc"));
        assert!(!is_plausible_debug_id(&"a".repeat(1024)));
    }

    #[tokio::test]
    async fn upsert_does_not_overwrite_other_project() {
        let pool = open_test_pool().await;
        store_sourcemap(&pool, &entry("beef", "a.js"), 1)
            .await
            .unwrap();
        store_sourcemap(&pool, &entry("beef", "b.js"), 2)
            .await
            .unwrap();

        let sm1 = load_sourcemap(&pool, "beef", 1).await.unwrap().unwrap();
        let sm2 = load_sourcemap(&pool, "beef", 2).await.unwrap().unwrap();
        assert_eq!(sm1.get_source(0), Some("a.js"));
        assert_eq!(sm2.get_source(0), Some("b.js"));
    }
}
