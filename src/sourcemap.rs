//! Sourcemap storage and resolution. Handles artifact bundle parsing
//! (ZIP files from `sentry-cli sourcemaps upload`) and on-the-fly
//! stack frame resolution using debug IDs.

use anyhow::{Context, Result};
use std::io::Read;

// ── Bundle parsing limits ───────────────────────────────────────────
// The ZIP central directory is attacker-controlled; never trust its
// declared sizes without a hard cap.

const MAX_BUNDLE_ENTRIES: usize = 10_000;
const MAX_BUNDLE_ENTRY_BYTES: usize = 64 * 1024 * 1024; // 64 MiB per entry

// ── Types ───────────────────────────────────────────────────────────

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

// ── Artifact bundle parsing ─────────────────────────────────────────

/// Extract sourcemaps from an artifact bundle ZIP.
///
/// The bundle is produced by `sentry-cli sourcemaps upload` and contains
/// a manifest plus the actual `.map` files. The manifest maps debug IDs
/// to source file paths.
///
/// ZIP decoding and JSON parsing are CPU-bound, so we offload to a blocking
/// task to keep the async runtime responsive on large uploads.
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

    // Try to find the manifest — could be at the root or under artifact-bundle/
    let manifest: serde_json::Value = try_read_manifest(&mut archive)?;

    let mut entries = Vec::new();

    // The manifest has a "files" object mapping file paths to metadata
    if let Some(files) = manifest.get("files").and_then(|f| f.as_object()) {
        for (zip_path, meta) in files {
            // Only care about sourcemap entries
            let file_type = meta.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if file_type != "source_map" && file_type != "sourcemap" {
                continue;
            }

            // The debug_id can be in headers or at the top level
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

            // Read the actual .map file from the ZIP
            let data = match read_zip_entry(&mut archive, zip_path) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!("skipping {zip_path}: {e}");
                    continue;
                }
            };

            entries.push(SourcemapEntry {
                debug_id,
                source_url,
                data,
            });
        }
    }

    // Fallback: if no manifest or no entries found, scan for .map files
    // that have debug_id embedded in the JSON
    if entries.is_empty() {
        entries = scan_for_sourcemaps(&mut archive)?;
    }

    Ok(entries)
}

fn try_read_manifest(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
) -> Result<serde_json::Value> {
    // Try both possible manifest locations
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
    // Check headers.debug-id first
    if let Some(id) = meta
        .get("headers")
        .and_then(|h| h.get("debug-id"))
        .and_then(|v| v.as_str())
    {
        return Some(normalize_debug_id(id));
    }
    // Then top-level debug_id / debugId
    if let Some(id) = meta
        .get("debug_id")
        .or_else(|| meta.get("debugId"))
        .and_then(|v| v.as_str())
    {
        return Some(normalize_debug_id(id));
    }
    None
}

/// Normalize debug IDs — strip the optional `-sourcemap` suffix
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
    let mut file = archive.by_name(name)?;
    // `file.size()` comes from the ZIP central directory and is attacker-controlled,
    // so clamp the preallocation. The post-read check catches under-declared sizes too.
    let cap = file.size().min(MAX_BUNDLE_ENTRY_BYTES as u64) as usize;
    let mut buf = Vec::with_capacity(cap);
    file.read_to_end(&mut buf)?;
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

        // Try to extract debug_id from the sourcemap JSON
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

// ── Frame resolution ────────────────────────────────────────────────

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

    // Try to get source content for context lines
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

// ── DB helpers ──────────────────────────────────────────────────────

use crate::db::{sql, translate_sql, DbPool};
use sqlx::Row;

/// Store a sourcemap entry (zstd-compressed) in the database.
pub async fn store_sourcemap(pool: &DbPool, entry: &SourcemapEntry, project_id: u64) -> Result<()> {
    let compressed =
        zstd::encode_all(entry.data.as_slice(), 3).context("zstd compress sourcemap")?;

    sqlx::query(sql!(
        "INSERT INTO sourcemaps (debug_id, source_url, data, project_id)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (debug_id) DO UPDATE SET data = ?3, source_url = ?2"
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
    let query = translate_sql(&query);

    let mut q = sqlx::query_scalar::<_, String>(&query);
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

/// Read chunks in order and concatenate them into a single buffer.
pub async fn assemble_chunks(
    pool: &DbPool,
    checksums: &[String],
    project_id: u64,
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

/// Load and decompress a sourcemap by debug_id, then parse it.
pub async fn load_sourcemap(pool: &DbPool, debug_id: &str) -> Result<Option<sourcemap::SourceMap>> {
    let row = sqlx::query(sql!("SELECT data FROM sourcemaps WHERE debug_id = ?1"))
        .bind(debug_id)
        .fetch_optional(pool)
        .await?;

    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    let compressed: Vec<u8> = row.get("data");
    let raw = zstd::decode_all(compressed.as_slice()).context("zstd decompress sourcemap")?;
    let sm = sourcemap::SourceMap::from_slice(&raw).context("parse sourcemap")?;

    Ok(Some(sm))
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
