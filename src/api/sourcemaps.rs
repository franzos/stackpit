use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::server::AppState;
use crate::sourcemap;

/// `GET /api/0/organizations/{org}/chunk-upload/`
///
/// Returns upload configuration. `sentry-cli` calls this first to learn
/// chunk size, hash algorithm, and where to POST chunks.
pub async fn chunk_upload_config(
    State(state): State<AppState>,
    Path(org): Path<String>,
    req: axum::http::Request<axum::body::Body>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
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
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let key_project_id = super::validate_api_key(&state.pool, &headers, "sourcemap").await?;
    let pool = &state.sourcemap_pool;
    let mut count = 0u32;

    loop {
        match multipart.next_field().await {
            Ok(Some(field)) => {
                let data = field.bytes().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(json!({ "detail": format!("failed to read chunk: {e}") })),
                    )
                })?;

                let checksum = sha1_hex(&data);
                sourcemap::store_chunk(pool, &checksum, &data, key_project_id)
                    .await
                    .map_err(|e| {
                        tracing::error!("store chunk: {e}");
                        super::internal_error(e)
                    })?;

                count += 1;
            }
            Ok(None) => break,
            Err(e) => {
                tracing::warn!("multipart parse error: {e}");
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "detail": format!("failed to read chunk: {e}") })),
                ));
            }
        }
    }

    tracing::debug!("stored {count} chunks");
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
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let key_project_id = super::validate_api_key(&state.pool, &headers, "sourcemap").await?;
    let pool = &state.sourcemap_pool;

    // Resolve the project ID from the request, falling back to the key's project
    let project_id = match resolve_project_id(&body.projects) {
        0 => key_project_id,
        id if id == key_project_id => id,
        _ => {
            return Err(super::api_error(
                StatusCode::FORBIDDEN,
                "API key not valid for this project",
            ))
        }
    };

    // Validate chunk checksums (must be 40-char lowercase hex SHA1)
    if body.chunks.len() > 128 {
        return Err(super::api_error(StatusCode::BAD_REQUEST, "too many chunks"));
    }
    for checksum in &body.chunks {
        if checksum.len() != 40 || !checksum.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(super::api_error(
                StatusCode::BAD_REQUEST,
                "invalid chunk checksum",
            ));
        }
    }

    // Check which chunks are already uploaded — return missing ones so
    // sentry-cli can upload them before retrying the assemble call.
    let missing = sourcemap::find_missing_chunks(pool, &body.chunks, project_id)
        .await
        .map_err(|e| {
            tracing::error!("check missing chunks: {e}");
            super::internal_error(e)
        })?;

    if !missing.is_empty() {
        return Ok(Json(json!({
            "state": "not_found",
            "missingChunks": missing,
        })));
    }

    // All chunks present — concatenate into the full artifact bundle
    let zip_data = sourcemap::assemble_chunks(pool, &body.chunks, project_id)
        .await
        .map_err(|e| {
            tracing::error!("assemble chunks: {e}");
            super::internal_error(e)
        })?;

    // Verify bundle integrity
    if let Some(ref expected) = body.checksum {
        let actual = sha1_hex(&zip_data);
        if actual != *expected {
            return Err(super::api_error(
                StatusCode::BAD_REQUEST,
                "bundle checksum mismatch",
            ));
        }
    }

    // Parse the artifact bundle and extract sourcemaps
    let entries = sourcemap::parse_artifact_bundle(&zip_data).map_err(|e| {
        tracing::error!("parse artifact bundle: {e}");
        super::internal_error(e)
    })?;

    let stored = entries.len();
    for entry in &entries {
        if let Err(e) = sourcemap::store_sourcemap(pool, entry, project_id).await {
            tracing::error!("store sourcemap {}: {e}", entry.debug_id);
        }
    }

    // Clean up the chunks we consumed
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
