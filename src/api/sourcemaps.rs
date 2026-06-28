use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::ingest::sourcemap;
use crate::server::AppState;

// Request limits (sync with chunk_upload_config maxRequestSize)
const MAX_CHUNK_FIELDS: usize = 128;
const MAX_CHUNK_TOTAL_BYTES: usize = 32 * 1024 * 1024;

/// GET /api/0/organizations/{org}/chunk-upload/ returns upload config.
pub async fn chunk_upload_config(
    State(state): State<AppState>,
    Path(org): Path<String>,
    req: axum::http::Request<axum::body::Body>,
) -> Result<impl IntoResponse, super::ApiError> {
    super::validate_api_key(&state.pool, req.headers(), "sourcemap").await?;
    Ok(Json(json!({
        "url": format!("/api/0/organizations/{org}/chunk-upload/"),
        "chunkSize": 8_388_608,
        "chunksPerRequest": 64,
        "maxFileSize": 2_147_483_648_u64,
        "maxRequestSize": 33_554_432,
        "concurrency": 1,
        "hashAlgorithm": "sha1",
        "accept": [
            "debug_files",
            "release_files",
            "pdbs",
            "sources",
            "bcsymbolmaps",
            "il2cpp",
            "portablepdbs",
            "artifact_bundles"
        ]
    })))
}

/// `POST /api/0/organizations/{org}/chunk-upload/`
///
/// Accepts multipart uploads of binary chunks. Each chunk is stored
/// keyed by its SHA1 checksum for later assembly.
pub async fn chunk_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, super::ApiError> {
    let key_project_id = super::validate_api_key(&state.pool, &headers, "sourcemap").await?;
    let pool = &state.sourcemap_pool;
    let mut count = 0u32;
    let mut total_bytes: usize = 0;

    loop {
        match multipart.next_field().await {
            Ok(Some(field)) => {
                if count as usize >= MAX_CHUNK_FIELDS {
                    tracing::warn!(
                        "chunk-upload field limit reached ({MAX_CHUNK_FIELDS}), rejecting request"
                    );
                    return Err(super::ApiError::new(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "too many multipart fields",
                    ));
                }

                let data = field.bytes().await.map_err(|e| {
                    super::ApiError::new(
                        StatusCode::BAD_REQUEST,
                        format!("failed to read chunk: {e}"),
                    )
                })?;

                total_bytes = total_bytes.saturating_add(data.len());
                if total_bytes > MAX_CHUNK_TOTAL_BYTES {
                    tracing::warn!(
                        "chunk-upload byte limit reached ({total_bytes} > {MAX_CHUNK_TOTAL_BYTES})"
                    );
                    return Err(super::ApiError::new(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "request body too large",
                    ));
                }

                let checksum = sha1_hex(&data);
                sourcemap::store_chunk(pool, &checksum, &data, key_project_id)
                    .await
                    .map_err(|e| {
                        tracing::error!("store chunk: {e}");
                        super::ApiError::internal(e)
                    })?;

                count += 1;
            }
            Ok(None) => break,
            Err(e) => {
                tracing::warn!("multipart parse error: {e}");
                return Err(super::ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!("failed to read chunk: {e}"),
                ));
            }
        }
    }

    tracing::debug!("stored {count} chunks ({total_bytes} bytes)");
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct AssembleRequest {
    pub checksum: Option<String>,
    pub chunks: Vec<String>,
    pub projects: Option<Vec<serde_json::Value>>,
}

/// `POST /api/0/organizations/{org}/artifactbundle/assemble/`
///
/// Assembles previously uploaded chunks into an artifact bundle (ZIP),
/// extracts sourcemaps by debug ID, and stores them.
pub async fn assemble(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AssembleRequest>,
) -> Result<impl IntoResponse, super::ApiError> {
    let key_project_id = super::validate_api_key(&state.pool, &headers, "sourcemap").await?;
    let pool = &state.sourcemap_pool;

    let project_id = match resolve_project_id(&body.projects) {
        0 => key_project_id,
        id if id == key_project_id => id,
        _ => {
            return Err(super::ApiError::new(
                StatusCode::FORBIDDEN,
                "API key not valid for this project",
            ))
        }
    };

    // Chunk checksums must be 40-char hex SHA1.
    if body.chunks.len() > 128 {
        return Err(super::ApiError::new(
            StatusCode::BAD_REQUEST,
            "too many chunks",
        ));
    }
    for checksum in &body.chunks {
        if checksum.len() != 40 || !checksum.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(super::ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid chunk checksum",
            ));
        }
    }

    // Return missing chunks so sentry-cli uploads them before retrying assemble.
    let missing = sourcemap::find_missing_chunks(pool, &body.chunks, project_id)
        .await
        .map_err(|e| {
            tracing::error!("check missing chunks: {e}");
            super::ApiError::internal(e)
        })?;

    if !missing.is_empty() {
        return Ok(Json(json!({
            "state": "not_found",
            "missingChunks": missing,
        })));
    }

    let zip_data = sourcemap::assemble_chunks(pool, &body.chunks, project_id)
        .await
        .map_err(|e| {
            tracing::error!("assemble chunks: {e}");
            super::ApiError::internal(e)
        })?;

    if let Some(ref expected) = body.checksum {
        let actual = sha1_hex(&zip_data);
        if actual != *expected {
            return Err(super::ApiError::new(
                StatusCode::BAD_REQUEST,
                "bundle checksum mismatch",
            ));
        }
    }

    // Offloaded to a blocking task so ZIP decode + JSON parse don't stall the runtime.
    let entries = sourcemap::parse_artifact_bundle(zip_data)
        .await
        .map_err(|e| {
            tracing::error!("parse artifact bundle: {e}");
            super::ApiError::internal(e)
        })?;

    let stored = entries.len();
    for entry in &entries {
        if let Err(e) = sourcemap::store_sourcemap(pool, entry, project_id).await {
            tracing::error!("store sourcemap {}: {e}", entry.debug_id);
        }
    }

    if let Err(e) = sourcemap::delete_chunks(pool, &body.chunks, project_id).await {
        tracing::warn!("cleanup chunks: {e}");
    }

    tracing::info!("stored {stored} sourcemaps from artifact bundle");
    Ok(Json(json!({
        "state": "ok",
        "missingChunks": []
    })))
}

fn resolve_project_id(projects: &Option<Vec<serde_json::Value>>) -> u64 {
    projects
        .as_ref()
        .and_then(|p| p.first())
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(0)
}

fn sha1_hex(data: &[u8]) -> String {
    use sha1::Digest;
    let hash = sha1::Sha1::digest(data);
    hex::encode(hash)
}
