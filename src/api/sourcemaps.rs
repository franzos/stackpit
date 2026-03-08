use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
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
pub async fn chunk_upload_config(Path(org): Path<String>) -> impl IntoResponse {
    Json(json!({
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
            "artifact_bundles",
            "artifact_bundles_v2"
        ]
    }))
}

/// `POST /api/0/organizations/{org}/chunk-upload/`
///
/// Accepts multipart uploads of binary chunks. Each chunk is stored
/// keyed by its SHA1 checksum for later assembly.
pub async fn chunk_upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let pool = &state.pool;
    let mut count = 0u32;

    while let Ok(Some(field)) = multipart.next_field().await {
        let data = field.bytes().await.map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "detail": format!("failed to read chunk: {e}") })),
            )
        })?;

        let checksum = sha1_hex(&data);
        sourcemap::store_chunk(pool, &checksum, &data)
            .await
            .map_err(|e| {
                tracing::error!("store chunk: {e}");
                super::internal_error(e)
            })?;

        count += 1;
    }

    tracing::debug!("stored {count} chunks");
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct AssembleRequest {
    #[allow(dead_code)]
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
    Json(body): Json<AssembleRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let pool = &state.pool;

    // Resolve the project ID from the request
    let project_id = resolve_project_id(&body.projects);

    // Concatenate chunks into the full artifact bundle
    let zip_data = sourcemap::assemble_chunks(pool, &body.chunks)
        .await
        .map_err(|e| {
            tracing::error!("assemble chunks: {e}");
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "detail": format!("chunk assembly failed: {e}") })),
            )
        })?;

    // Parse the artifact bundle and extract sourcemaps
    let entries = sourcemap::parse_artifact_bundle(&zip_data).map_err(|e| {
        tracing::error!("parse artifact bundle: {e}");
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "detail": format!("invalid artifact bundle: {e}") })),
        )
    })?;

    let stored = entries.len();
    for entry in &entries {
        if let Err(e) = sourcemap::store_sourcemap(pool, entry, project_id).await {
            tracing::error!("store sourcemap {}: {e}", entry.debug_id);
        }
    }

    // Clean up the chunks we consumed
    if let Err(e) = sourcemap::delete_chunks(pool, &body.chunks).await {
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
