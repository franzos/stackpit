use crate::api;
use crate::auth_service::AuthCache;
use crate::config::Config;
use crate::crypto::SecretEncryptor;
use crate::db::{self, DbPool};
use crate::endpoints;
use crate::filter::FilterEngine;
use crate::html;
use crate::mcp::{McpRuntime, ResourceMetadata};
use crate::middleware;
use crate::oauth::OidcClient;
use crate::queries::filters::load_filter_data;
use crate::stats::{DiscardStats, IngestStats};
use crate::writer::{self, WriterHandle};
use anyhow::Result;
use axum::extract::Request;
use axum::routing::{get, post};
use axum::{Router, ServiceExt};
use stackpit_auth::{BearerGate, BearerGateConfig, JwksCache, JwtVerifierConfig};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};
use tower_http::decompression::RequestDecompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::normalize_path::NormalizePath;
use tower_http::timeout::TimeoutLayer;

/// Artifact-bundle uploads can be large; cap both the extractor (`DefaultBodyLimit`)
/// and the transport reader (`RequestBodyLimitLayer`) at the same ceiling.
const ARTIFACT_BUNDLE_BODY_LIMIT: usize = 64 * 1024 * 1024;

/// Shared application state. The `ReadPool` extractor is an ergonomic shortcut
/// that clones the read pool; it does not narrow access at the type level
/// (it yields the same `DbPool` as the write pools).
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub writer: WriterHandle,
    pub pool: DbPool,
    /// Shares the writer actor's pool so admin direct-writes serialise against
    /// the actor on the SQLite write lock (single connection).
    pub writer_pool: DbPool,
    /// Grant-vault writes from the web auth middleware (per request).
    pub auth_pool: DbPool,
    pub sourcemap_pool: DbPool,
    pub filter_engine: Arc<FilterEngine>,
    pub discard_stats: Arc<DiscardStats>,
    pub auth_cache: AuthCache,
    pub encryptor: Option<Arc<SecretEncryptor>>,
    #[allow(dead_code)]
    pub ingest_stats: Arc<IngestStats>,
    /// `Some` iff `[auth.oauth]` is configured and discovery succeeded.
    pub oidc: Option<Arc<OidcClient>>,
    /// Web gate: introspects the cookie-indexed grant's access token.
    pub web_bearer_gate: Option<BearerGate>,
    /// `Some` iff both `[auth.oauth]` and `[auth.mcp]` are configured.
    pub mcp: Option<Arc<McpRuntime>>,
}

async fn ingest_landing() -> axum::response::Html<&'static str> {
    axum::response::Html(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Stackpit — Ingest</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
         background: #0f1117; color: #c9d1d9; display: flex; align-items: center;
         justify-content: center; min-height: 100vh; }
  .card { max-width: 540px; padding: 2.5rem; text-align: center; }
  h1 { font-size: 1.8rem; color: #e6edf3; margin-bottom: 0.3rem; }
  .sub { color: #8b949e; font-size: 0.95rem; margin-bottom: 1.8rem; }
  .info { text-align: left; font-size: 0.85rem; line-height: 1.7; color: #8b949e; }
  .info strong { color: #c9d1d9; }
  hr { border: none; border-top: 1px solid #21262d; margin: 1.2rem 0; }
  .footer { font-size: 0.75rem; color: #484f58; margin-top: 1.5rem; }
  a { color: #58a6ff; text-decoration: none; }
  a:hover { text-decoration: underline; }
</style>
</head>
<body>
<div class="card">
  <h1>Stackpit</h1>
  <p class="sub">Sentry-compatible error tracking</p>
  <div class="info">
    <p><strong>This is the ingest port.</strong></p>
    <p>Sentry SDKs submit events here. Point your DSN at this address.</p>
    <hr>
    <p>The web UI and API are served on a separate port.</p>
    <p>Check <strong>/health</strong> for service status.</p>
  </div>
  <div class="footer">
    <p>Developed by <a href="https://gofranz.com/">Franz Geffke</a></p>
  </div>
</div>
</body>
</html>"#,
    )
}

/// Static liveness probe; metrics belong on an authed endpoint.
async fn health_handler() -> &'static str {
    "ok"
}

/// Public-ingest liveness probe; static body so it doesn't vary with traffic.
async fn ingest_health_handler() -> &'static str {
    "ok"
}

pub async fn run(config: Config, ingest_only: bool) -> Result<()> {
    let db_url = config.storage.database_url();

    // Run migrations once before creating pools
    let migration_pool = db::create_writer_pool(&db_url).await?;
    db::run_migrations(&migration_pool).await?;
    crate::oidc::grants::backfill_csrf_tokens(&migration_pool).await?;
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

    // Writer-side pools. SQLite: one single-connection pool per subsystem so a
    // slow background task can't head-of-line-block the writer on the lone write
    // connection. Postgres: real write concurrency, so a shared foreground pool
    // plus a small background pool is enough.
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let (writer_pool, bg_writer_pool, auth_pool, sourcemap_pool, mcp_writer_pool) = (
        db::create_writer_pool(&db_url).await?,
        db::create_writer_pool(&db_url).await?,
        db::create_writer_pool(&db_url).await?,
        db::create_writer_pool(&db_url).await?,
        db::create_writer_pool(&db_url).await?,
    );

    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    let (writer_pool, bg_writer_pool, auth_pool, sourcemap_pool, mcp_writer_pool) = {
        let writer = db::create_writer_pool(&db_url).await?;
        let bg = db::create_bg_pool(&db_url).await?;
        (writer.clone(), bg, writer.clone(), writer.clone(), writer)
    };

    let ingest_stats = Arc::new(IngestStats::new());

    // Admin direct-writes reuse the actor's pool (same SQLite write connection).
    let admin_writer_pool = writer_pool.clone();

    let (writer_tx, writer_join) =
        writer::spawn(writer_pool, Some(notify_tx), Arc::clone(&ingest_stats)).await?;

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
    // OIDC grants / revocation markers / JTI dedupe -- hourly purge.
    crate::background::spawn_oidc_cleanup_task(bg_writer_pool.clone(), bg_cancel.child_token());

    let auth_cache: AuthCache = Arc::new(dashmap::DashMap::new());

    let encryptor = SecretEncryptor::from_env()
        .map_err(anyhow::Error::msg)?
        .map(Arc::new);

    if encryptor.is_none() {
        let (count,): (i64,) = sqlx::query_as(db::sql!(
            "SELECT COUNT(*) FROM integrations WHERE encrypted = TRUE"
        ))
        .fetch_one(&pool)
        .await?;

        if count > 0 {
            anyhow::bail!(
                "STACKPIT_MASTER_KEY is not set but {count} integration(s) have encrypted secrets. \
                 Set STACKPIT_MASTER_KEY to the same key used when the secrets were created, \
                 otherwise those secrets cannot be decrypted and notifications will fail."
            );
        }

        tracing::warn!(
            "STACKPIT_MASTER_KEY is not set — integration secrets will be stored in plaintext. \
             Set a 64-character hex key to enable encryption."
        );
    }

    // Discover at startup. Failure → admin-token-only unless `required=true`.
    let oidc = if config.auth.oauth.is_enabled() {
        // OAuth stores IdP tokens server-side; refuse without the master key.
        if encryptor.is_none() {
            anyhow::bail!(
                "OAuth/OIDC is enabled but STACKPIT_MASTER_KEY is not set. \
                 Server-side tokens must be encrypted at rest; \
                 set STACKPIT_MASTER_KEY (64 hex chars) and restart."
            );
        }
        match OidcClient::discover(&config.auth.oauth, config.auth.mcp.jwks_cache_ttl_secs).await {
            Ok(c) => {
                tracing::info!("OIDC client ready (issuer discovery succeeded)");
                Some(Arc::new(c))
            }
            Err(e) if config.auth.oauth.required => {
                anyhow::bail!("OIDC discovery failed and auth.oauth.required=true: {e:#}");
            }
            Err(e) => {
                tracing::error!(
                    "OIDC discovery failed at startup: {e:#}. SSO disabled, admin-token only."
                );
                None
            }
        }
    } else {
        None
    };

    // Shared so back-channel logout evicts both surfaces in one write.
    let revocation_store = oidc
        .as_ref()
        .map(|_| crate::oidc::revocations::SqliteRevocationStore::new(auth_pool.clone()));

    // Built only when OAuth is live (same Hydra).
    let mcp = build_mcp_runtime(
        &config,
        oidc.as_ref(),
        mcp_writer_pool,
        revocation_store.clone(),
    )?;

    // Web gate: introspects the access token from the cookie-indexed grant row.
    let web_bearer_gate = oidc
        .as_ref()
        .and_then(|client| build_web_bearer_gate(client, &config, revocation_store.clone()));

    // Spawn notification dispatcher
    {
        let notify_pool = pool.clone();
        let notify_encryptor = encryptor.clone();
        let notify_config = config.clone();
        let notify_rate_limiter = Arc::new(crate::notify::rate_limit::NotifyRateLimiter::new(
            config.notifications.rate_limit_per_project,
            config.notifications.rate_limit_global,
        ));
        crate::notify::spawn_dispatcher(
            notify_rx,
            notify_pool,
            notify_encryptor,
            notify_config,
            notify_rate_limiter,
        );
    }

    let state = AppState {
        config: config.clone(),
        writer: writer_tx.clone(),
        pool: pool.clone(),
        writer_pool: admin_writer_pool,
        auth_pool: auth_pool.clone(),
        sourcemap_pool,
        filter_engine,
        discard_stats,
        auth_cache,
        encryptor,
        ingest_stats,
        oidc: oidc.clone(),
        web_bearer_gate: web_bearer_gate.clone(),
        mcp: mcp.clone(),
    };

    // Rate limiting: handled by filter engine at handler level.
    // CORS: permissive for SDKs. Body limit: 64MB for artifact bundles.
    let sentry_api = api::sentry_api_routes()
        .layer(axum::extract::DefaultBodyLimit::max(
            ARTIFACT_BUNDLE_BODY_LIMIT,
        ))
        .layer(RequestBodyLimitLayer::new(ARTIFACT_BUNDLE_BODY_LIMIT))
        .with_state(state.clone());

    // Tight limits for ingestion: compressed → decompress → decompressed.
    let ingest_routes = Router::new()
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
        .layer(RequestBodyLimitLayer::new(config.server.max_body_size))
        .layer(RequestDecompressionLayer::new())
        .layer(RequestBodyLimitLayer::new(
            config.server.compressed_body_limit(),
        ))
        .with_state(state.clone());

    // Origin stays wildcard -- ingest is genuinely cross-origin from arbitrary
    // customer apps -- but methods and headers are narrowed to the SDK surface.
    use axum::http::header::{
        HeaderName, AUTHORIZATION, CONTENT_ENCODING, CONTENT_TYPE, USER_AGENT,
    };
    use axum::http::Method;
    let ingest_cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::OPTIONS])
        .allow_headers([
            HeaderName::from_static("x-sentry-auth"),
            AUTHORIZATION,
            CONTENT_TYPE,
            CONTENT_ENCODING,
            USER_AGENT,
        ]);
    let cors_scoped = sentry_api.merge(ingest_routes).layer(ingest_cors);

    let ingest_router = Router::new()
        .route("/", get(ingest_landing))
        .route("/health", get(ingest_health_handler))
        .with_state(state.clone())
        .merge(cors_scoped)
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(30),
        ));

    // Outermost: rewrites the path before routing so Sentry's trailing-slash
    // requests match the canonical (slash-less) routes. Must wrap the whole
    // service; a `Router::layer` would run after routing, too late to help.
    let ingest_app = ServiceExt::<Request>::into_make_service_with_connect_info::<
        std::net::SocketAddr,
    >(NormalizePath::trim_trailing_slash(ingest_router));

    let ingest_bind = &config.server.ingest_bind;
    let ingest_listener = tokio::net::TcpListener::bind(ingest_bind).await?;

    if ingest_only {
        tracing::info!("ingestion listening on {ingest_bind} (ingest-only mode)");

        serve_with_shutdown(writer_tx, writer_join, bg_cancel, |tx| {
            let mut ingest_rx = tx.subscribe();
            let ingest_server =
                axum::serve(ingest_listener, ingest_app).with_graceful_shutdown(async move {
                    let _ = ingest_rx.wait_for(|&v| v).await;
                });
            vec![(
                "ingest",
                Box::pin(async move { ingest_server.await }) as ServerFuture,
            )]
        })
        .await;
    } else {
        let rate_limiter = middleware::new_rate_limiter_state();

        let admin_app = Router::new()
            .route("/health", get(health_handler))
            .merge(api::routes())
            .merge(html::routes())
            .layer(RequestBodyLimitLayer::new(config.server.max_body_size))
            .layer(RequestDecompressionLayer::new())
            .layer(RequestBodyLimitLayer::new(
                config.server.compressed_body_limit(),
            ))
            .layer(axum::middleware::from_fn_with_state(
                middleware::CsrfConfig {
                    max_body_size: config.server.max_body_size,
                },
                middleware::csrf_middleware,
            ))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::web_auth_middleware,
            ));

        // MCP router: bearer auth, no cookies. Merged only when OAuth enabled.
        let mcp_app = mcp.as_deref().map(crate::mcp::routes).unwrap_or_default();

        let admin_app = admin_app
            .merge(mcp_app)
            .layer(axum::middleware::from_fn_with_state(
                rate_limiter,
                middleware::rate_limit_middleware,
            ))
            .layer(axum::middleware::from_fn(
                middleware::security_headers_middleware,
            ))
            .with_state(state);

        let admin_bind = &config.server.bind;
        let admin_listener = tokio::net::TcpListener::bind(admin_bind).await?;

        tracing::info!("ingestion listening on {ingest_bind}");
        tracing::info!("admin listening on {admin_bind}");
        if mcp.is_some() {
            tracing::info!("mcp endpoint enabled at /mcp");
        }

        serve_with_shutdown(writer_tx, writer_join, bg_cancel, |tx| {
            let mut ingest_rx = tx.subscribe();
            let mut admin_rx = tx.subscribe();
            let ingest_server =
                axum::serve(ingest_listener, ingest_app).with_graceful_shutdown(async move {
                    let _ = ingest_rx.wait_for(|&v| v).await;
                });
            let admin_server =
                axum::serve(admin_listener, admin_app).with_graceful_shutdown(async move {
                    let _ = admin_rx.wait_for(|&v| v).await;
                });
            vec![
                (
                    "ingest",
                    Box::pin(async move { ingest_server.await }) as ServerFuture,
                ),
                (
                    "admin",
                    Box::pin(async move { admin_server.await }) as ServerFuture,
                ),
            ]
        })
        .await;
    }

    Ok(())
}

type ServerFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + Send>>;

/// Runs one or two `axum::serve` futures behind a single shared watch-channel
/// shutdown signal, with a 5s force-exit ceiling once draining begins.
///
/// `build_servers` receives the sender so each server can subscribe its own
/// graceful-shutdown receiver before the signal task is spawned.
async fn serve_with_shutdown<F>(
    writer_tx: WriterHandle,
    writer_join: tokio::task::JoinHandle<()>,
    bg_cancel: CancellationToken,
    build_servers: F,
) where
    F: FnOnce(&tokio::sync::watch::Sender<bool>) -> Vec<(&'static str, ServerFuture)>,
{
    let (shutdown_tx, _) = tokio::sync::watch::channel(false);
    let servers = build_servers(&shutdown_tx);
    let mut timeout_rx = shutdown_tx.subscribe();

    tokio::spawn(async move {
        shutdown_signal(writer_tx, writer_join, bg_cancel).await;
        let _ = shutdown_tx.send(true);
    });

    let mut server_set = tokio::task::JoinSet::new();
    for (label, server) in servers {
        server_set.spawn(async move {
            if let Err(e) = server.await {
                tracing::error!("{label} server error: {e}");
            }
        });
    }

    // First server to finish (graceful or error) ends the run, mirroring the
    // prior `select!` over the individual server futures.
    tokio::select! {
        _ = server_set.join_next() => {}
        _ = async {
            let _ = timeout_rx.wait_for(|&v| v).await;
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            tracing::warn!("graceful shutdown timed out after 5s, forcing exit");
        } => {}
    }
}

/// Requires `[auth.oauth]` (issuer + JWKS) and `[auth.mcp].audience`.
/// `Ok(None)` = MCP not configured; `Err` = configured but no validation path.
fn build_mcp_runtime(
    config: &Config,
    oidc: Option<&Arc<OidcClient>>,
    writer_pool: DbPool,
    revocation_store: Option<crate::oidc::revocations::SqliteRevocationStore>,
) -> Result<Option<Arc<McpRuntime>>> {
    if !config.auth.mcp.is_enabled() {
        return Ok(None);
    }
    let Some(oidc) = oidc else {
        return Ok(None);
    };

    let mcp = &config.auth.mcp;
    let oauth = &config.auth.oauth;

    let audience = mcp
        .audience
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("auth.mcp.audience is required"))?
        .to_string();
    let issuer = oauth
        .issuer_url
        .as_deref()
        .ok_or_else(|| {
            anyhow::anyhow!("auth.oauth.issuer_url is required when auth.mcp is enabled")
        })?
        .to_string();
    let client_id = oauth
        .client_id
        .as_deref()
        .ok_or_else(|| {
            anyhow::anyhow!("auth.oauth.client_id is required when auth.mcp is enabled")
        })?
        .to_string();

    // Precedence: explicit `auth.mcp.introspection_url` → discovery → none.
    let introspection_url = mcp
        .introspection_url
        .as_deref()
        .or_else(|| oidc.introspection_endpoint())
        .map(str::to_string);

    // Reuse OidcClient's cache by default; explicit `auth.mcp.jwks_url`
    // override gets its own cache but shares the OIDC HTTP client pool.
    let jwks_cache = match mcp.jwks_url.as_deref() {
        Some(url) if url != oidc.jwks_uri() => Some(JwksCache::new(
            oidc.http_client(),
            url.to_string(),
            mcp.jwks_cache_ttl_secs,
        )),
        _ if oidc.jwks_uri().is_empty() => None,
        _ => Some(oidc.jwks_cache().clone()),
    };

    if jwks_cache.is_none() && introspection_url.is_none() {
        anyhow::bail!(
            "auth.mcp is enabled but neither a JWKS endpoint nor an introspection endpoint \
             is configured. Set auth.mcp.introspection_url, configure Hydra to advertise \
             jwks_uri / introspection_endpoint in its discovery doc, or disable [auth.mcp]."
        );
    }

    // Well-known absolute URL: prefer external_url (proxy-aware).
    let resource_base = config
        .server
        .external_url
        .as_deref()
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|| audience.trim_end_matches('/').to_string());
    let resource_metadata_url = format!("{resource_base}/.well-known/oauth-protected-resource");

    // Same `users` table for MCP + browser-OIDC.
    let provisioner: Arc<dyn stackpit_auth::UserProvisioner> =
        Arc::new(crate::mcp::DbProvisioner::new(writer_pool));

    let gate = BearerGate::with_client(
        oidc.http_client(),
        BearerGateConfig {
            introspection_url,
            audience: audience.clone(),
            resource_metadata_url,
            realm: "stackpit".to_string(),
            expected_issuer: Some(issuer.clone()),
            client_id,
            // Break-glass so operators can poke /mcp without an OAuth client.
            admin_token: config.server.admin_token.clone(),
            introspection_client_id: mcp.introspection_client_id.clone(),
            introspection_client_secret: mcp.introspection_client_secret.clone(),
            cache_ttl_secs: mcp.effective_cache_ttl_secs(),
            cache_max_ttl_secs: mcp.cache_max_ttl_secs,
            provisioner: Some(provisioner),
            revocation: revocation_store.map(|r| r.into_arc()),
            jwt: jwks_cache.map(|jwks| JwtVerifierConfig { jwks }),
        },
    );
    let metadata = Arc::new(ResourceMetadata::new(&audience, &issuer));

    Ok(Some(Arc::new(McpRuntime { metadata, gate })))
}

/// Introspects the access token from the grant vault per request. `None`
/// when no introspection endpoint is resolvable from oauth/mcp/discovery.
fn build_web_bearer_gate(
    oidc: &Arc<OidcClient>,
    config: &Config,
    revocation_store: Option<crate::oidc::revocations::SqliteRevocationStore>,
) -> Option<BearerGate> {
    let introspection_url = config
        .auth
        .oauth
        .introspection_url
        .as_deref()
        .or(config.auth.mcp.introspection_url.as_deref())
        .or_else(|| oidc.introspection_endpoint())
        .map(str::to_string)?;
    let issuer = config.auth.oauth.issuer_url.as_deref()?.to_string();
    let client_id = config
        .auth
        .oauth
        .client_id
        .as_deref()
        .unwrap_or("")
        .to_string();

    Some(BearerGate::with_client(
        oidc.http_client(),
        BearerGateConfig {
            introspection_url: Some(introspection_url),
            audience: config.auth.oauth.web_audience.clone(),
            resource_metadata_url: String::new(),
            realm: String::new(),
            expected_issuer: Some(issuer),
            client_id,
            // web_auth_middleware enforces admin_token ahead of the gate.
            admin_token: None,
            introspection_client_id: config.auth.mcp.introspection_client_id.clone(),
            introspection_client_secret: config.auth.mcp.introspection_client_secret.clone(),
            cache_ttl_secs: config.auth.oauth.introspection_cache_ttl_secs,
            cache_max_ttl_secs: config.auth.oauth.cache_max_ttl_secs,
            // Callback upserts the user row before issuing the grant.
            provisioner: None,
            revocation: revocation_store.map(|r| r.into_arc()),
            // Web BFF accepts only opaque cookie-resolved tokens.
            jwt: None,
        },
    ))
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

    // Wait for writer drain (4s timeout leaves headroom before force-exit).
    match tokio::time::timeout(std::time::Duration::from_secs(4), writer_join).await {
        Ok(Ok(())) => tracing::info!("writer drained successfully"),
        Ok(Err(e)) => tracing::error!("writer task panicked: {e}"),
        Err(_) => tracing::warn!("writer drain timed out after 4s"),
    }
}
