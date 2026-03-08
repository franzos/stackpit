use crate::api;
use crate::auth_service::AuthCache;
use crate::config::Config;
use crate::crypto::SecretEncryptor;
use crate::db::{self, DbPool};
use crate::endpoints;
use crate::filter::FilterEngine;
use crate::html;
use crate::middleware;
use crate::queries::filters::load_filter_data;
use crate::stats::{DiscardStats, IngestStats};
use crate::writer::{self, WriterHandle};
use anyhow::Result;
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};
use tower_http::decompression::RequestDecompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

/// Shared application state passed to all Axum handlers.
///
/// Field usage by handler category:
///
/// - **Read-only HTML** (`html::*` GET handlers): `pool` (via `ReadPool` extractor)
/// - **Write HTML** (`html::*` POST handlers): `writer` + `pool`, sometimes
///   `config` (DSN), `encryptor` (secret encryption), `auth_cache` (invalidation)
/// - **Ingestion endpoints** (`endpoints::*`): `writer`, `filter_engine`,
///   `discard_stats`, `auth_cache`
/// - **API read** (`api::*` GET): `pool` (via `ApiReadPool` extractor)
/// - **API write** (`api::*` POST/DELETE): `writer` + `pool`
///
/// The extractors (`ReadPool`, `ApiReadPool`) already narrow access for read
/// paths — handlers only see a `DbPool`, not the full `AppState`.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub writer: WriterHandle,
    pub pool: DbPool,
    pub filter_engine: Arc<FilterEngine>,
    pub discard_stats: Arc<DiscardStats>,
    pub auth_cache: AuthCache,
    pub encryptor: Option<Arc<SecretEncryptor>>,
    pub ingest_stats: Arc<IngestStats>,
}

async fn health_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl axum::response::IntoResponse {
    use std::sync::atomic::Ordering;

    let stats = &state.ingest_stats;
    axum::Json(serde_json::json!({
        "status": "ok",
        "events": {
            "accepted": stats.events_accepted.load(Ordering::Relaxed),
            "rejected": stats.events_rejected.load(Ordering::Relaxed),
            "dropped": stats.events_dropped.load(Ordering::Relaxed),
        },
        "writer": {
            "queue_used": state.writer.queue_used(),
            "queue_capacity": state.writer.queue_max(),
        }
    }))
}

pub async fn run(config: Config, ingest_only: bool) -> Result<()> {
    let db_url = config.storage.database_url();

    // Run migrations once before creating pools
    let migration_pool = db::create_writer_pool(&db_url).await?;
    db::run_migrations(&migration_pool).await?;
    drop(migration_pool);

    // Create pools
    let pool = db::create_pool(&db_url).await?;

    let config = Arc::new(config);

    // Load initial filter data before constructing the engine
    let initial_data = match load_filter_data(&pool).await {
        Ok(data) => data,
        Err(e) => {
            tracing::warn!("filter engine: initial load failed (fail-open): {e}");
            crate::filter::FilterData::default()
        }
    };

    // FilterEngine must be created before writer::spawn so the writer can reference it
    let filter_engine = Arc::new(FilterEngine::new(
        initial_data,
        config.filter.rate_limit,
        config.filter.excluded_environments.clone(),
        config.filter.blocked_user_agents.clone(),
    ));

    // Notification channel + dispatcher
    let (notify_tx, notify_rx) = tokio::sync::mpsc::channel(1000);
    let digest_notify_tx = notify_tx.clone();

    // Create a separate writer pool (max_connections=1 for SQLite)
    let writer_pool = db::create_writer_pool(&db_url).await?;

    // Background tasks that write (retention, discard stats, WAL checkpoint)
    // get their own writer pool so they use the correct PRAGMAs and don't
    // contend with the read pool for SQLite's single write lock.
    let bg_writer_pool = db::create_writer_pool(&db_url).await?;

    let ingest_stats = Arc::new(IngestStats::new());

    let (writer_tx, writer_join) = writer::spawn(
        writer_pool,
        Some(Arc::clone(&filter_engine)),
        Some(notify_tx),
        Arc::clone(&ingest_stats),
    )
    .await?;

    let discard_stats = Arc::new(DiscardStats::new());

    let bg_cancel = CancellationToken::new();
    crate::background::spawn_retention_task(
        bg_writer_pool.clone(),
        config.storage.retention_days,
        bg_cancel.child_token(),
    );
    crate::background::spawn_discard_stats_task(
        bg_writer_pool.clone(),
        Arc::clone(&discard_stats),
        bg_cancel.child_token(),
    );
    crate::background::spawn_wal_checkpoint_task(bg_writer_pool.clone(), bg_cancel.child_token());
    crate::background::spawn_digest_task(pool.clone(), digest_notify_tx, bg_cancel.child_token());

    let auth_cache: AuthCache = Arc::new(dashmap::DashMap::new());

    let encryptor = SecretEncryptor::from_env().map(Arc::new);
    if encryptor.is_none() {
        tracing::warn!(
            "STACKPIT_MASTER_KEY not set — integration secrets will be stored in plaintext"
        );
    }

    // Spawn notification dispatcher
    {
        let notify_pool = pool.clone();
        let notify_encryptor = encryptor.clone();
        let notify_rate_limiter = Arc::new(crate::notify::rate_limit::NotifyRateLimiter::new(
            config.notifications.rate_limit_per_project,
            config.notifications.rate_limit_global,
        ));
        tokio::spawn(crate::notify::run_dispatcher(
            notify_rx,
            notify_pool,
            notify_encryptor,
            notify_rate_limiter,
        ));
    }

    let state = AppState {
        config: config.clone(),
        writer: writer_tx.clone(),
        pool,
        filter_engine,
        discard_stats,
        auth_cache,
        encryptor,
        ingest_stats,
    };

    // Ingestion rate limiting is handled at the application level by the filter
    // engine's `pre_filter_check` (called in each endpoint handler before body
    // parsing), so no additional rate-limit layer is needed here.

    // Ingestion routes get permissive CORS (SDKs send from any origin)
    let ingest_app = Router::new()
        .route("/health", get(health_handler))
        .route(
            "/api/{project_id}/envelope/",
            post(endpoints::envelope::handle),
        )
        .route(
            "/api/{project_id}/envelope",
            post(endpoints::envelope::handle),
        )
        .route("/api/{project_id}/store/", post(endpoints::store::handle))
        .route("/api/{project_id}/store", post(endpoints::store::handle))
        .route(
            "/api/{project_id}/security/",
            post(endpoints::security::handle),
        )
        .route(
            "/api/{project_id}/security",
            post(endpoints::security::handle),
        )
        .route(
            "/api/{project_id}/minidump/",
            post(endpoints::minidump::handle),
        )
        .route(
            "/api/{project_id}/minidump",
            post(endpoints::minidump::handle),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(30),
        ))
        // Layers apply bottom-up (last-added = outermost = runs first).
        // Request flow: compressed-limit → decompress → decompressed-limit → handler
        .layer(RequestBodyLimitLayer::new(config.server.max_body_size))
        .layer(RequestDecompressionLayer::new())
        .layer(RequestBodyLimitLayer::new(
            config.server.compressed_body_limit(),
        ))
        .with_state(state.clone())
        .into_make_service_with_connect_info::<std::net::SocketAddr>();

    let ingest_bind = &config.server.ingest_bind;
    let ingest_listener = tokio::net::TcpListener::bind(ingest_bind).await?;

    if ingest_only {
        tracing::info!("ingestion listening on {ingest_bind} (ingest-only mode)");

        let (shutdown_tx, mut shutdown_rx1) = tokio::sync::watch::channel(false);
        let mut shutdown_rx2 = shutdown_tx.subscribe();

        let bg_cancel_clone = bg_cancel.clone();
        tokio::spawn(async move {
            shutdown_signal(writer_tx, writer_join, bg_cancel_clone).await;
            let _ = shutdown_tx.send(true);
        });

        let server = axum::serve(ingest_listener, ingest_app).with_graceful_shutdown(async move {
            let _ = shutdown_rx1.wait_for(|&v| v).await;
        });

        tokio::select! {
            res = server => { if let Err(e) = res { tracing::error!("ingest server error: {e}"); } }
            _ = async {
                let _ = shutdown_rx2.wait_for(|&v| v).await;
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                tracing::warn!("graceful shutdown timed out after 5s, forcing exit");
            } => {}
        }
    } else {
        let admin_token = config.server.admin_token.clone();
        let use_secure_cookies = config
            .server
            .external_url
            .as_ref()
            .is_some_and(|u| u.starts_with("https://"));
        let rate_limiter = middleware::new_rate_limiter_state();
        let admin_app = Router::new()
            .merge(api::routes())
            .merge(html::routes())
            .layer(RequestBodyLimitLayer::new(config.server.max_body_size))
            .layer(RequestDecompressionLayer::new())
            .layer(RequestBodyLimitLayer::new(
                config.server.compressed_body_limit(),
            ))
            .layer(axum::middleware::from_fn_with_state(
                use_secure_cookies,
                middleware::csrf_middleware,
            ))
            .layer(axum::middleware::from_fn_with_state(
                admin_token,
                middleware::admin_auth_middleware,
            ))
            .layer(axum::middleware::from_fn_with_state(
                rate_limiter,
                middleware::rate_limit_middleware,
            ))
            .with_state(state);

        let admin_bind = &config.server.bind;
        let admin_listener = tokio::net::TcpListener::bind(admin_bind).await?;

        tracing::info!("ingestion listening on {ingest_bind}");
        tracing::info!("admin listening on {admin_bind}");

        let (shutdown_tx, mut shutdown_rx1) = tokio::sync::watch::channel(false);
        let mut shutdown_rx2 = shutdown_tx.subscribe();
        let mut shutdown_rx3 = shutdown_tx.subscribe();

        let bg_cancel_clone = bg_cancel.clone();
        tokio::spawn(async move {
            shutdown_signal(writer_tx, writer_join, bg_cancel_clone).await;
            let _ = shutdown_tx.send(true);
        });

        let ingest_server =
            axum::serve(ingest_listener, ingest_app).with_graceful_shutdown(async move {
                let _ = shutdown_rx1.wait_for(|&v| v).await;
            });

        let admin_server =
            axum::serve(admin_listener, admin_app).with_graceful_shutdown(async move {
                let _ = shutdown_rx2.wait_for(|&v| v).await;
            });

        tokio::select! {
            res = ingest_server => { if let Err(e) = res { tracing::error!("ingest server error: {e}"); } }
            res = admin_server => { if let Err(e) = res { tracing::error!("admin server error: {e}"); } }
            _ = async {
                let _ = shutdown_rx3.wait_for(|&v| v).await;
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                tracing::warn!("graceful shutdown timed out after 5s, forcing exit");
            } => {}
        }
    }

    Ok(())
}

async fn shutdown_signal(
    writer: WriterHandle,
    writer_join: tokio::task::JoinHandle<()>,
    bg_cancel: CancellationToken,
) {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("failed to install Ctrl+C handler: {e}");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                tracing::error!("failed to install SIGTERM handler: {e}");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received, draining writer...");
    bg_cancel.cancel();
    let _ = writer.shutdown();

    // Wait for the writer to finish draining its channel before signalling
    // the servers to stop. 4s timeout leaves headroom before the 5s force-exit.
    match tokio::time::timeout(std::time::Duration::from_secs(4), writer_join).await {
        Ok(Ok(())) => tracing::info!("writer drained successfully"),
        Ok(Err(e)) => tracing::error!("writer task panicked: {e}"),
        Err(_) => tracing::warn!("writer drain timed out after 4s"),
    }
}
