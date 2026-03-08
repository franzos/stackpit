use crate::config::Config;
use crate::db;
use anyhow::Result;
use sqlx::Row;
pub async fn run(config: &Config) -> Result<()> {
    let database_url = config.storage.database_url();
    let is_postgres =
        database_url.starts_with("postgres://") || database_url.starts_with("postgresql://");
    let backend = if is_postgres { "PostgreSQL" } else { "SQLite" };

    // --- Server ---

    println!("\x1b[1m  Server\x1b[0m");
    println!();
    println!("    Admin UI        {}", config.server.bind);
    println!("    Ingestion       {}", config.server.ingest_bind);
    if let Some(ref url) = config.server.external_url {
        println!("    External URL    {url}");
    } else {
        println!(
            "    External URL    \x1b[2m(not set -- DSN will use http://{})\x1b[0m",
            config.server.ingest_bind
        );
    }
    println!(
        "    Max body size   {}",
        format_bytes(config.server.max_body_size)
    );

    // --- Security ---

    println!();
    println!("\x1b[1m  Security\x1b[0m");
    println!();

    if config.server.admin_token.is_some() {
        println!("    Admin auth      \x1b[32menabled\x1b[0m");
    } else {
        let addr: std::net::SocketAddr = config
            .server
            .bind
            .parse()
            .unwrap_or_else(|_| "127.0.0.1:3000".parse().unwrap());
        if addr.ip().is_loopback() {
            println!(
                "    Admin auth      \x1b[33mdisabled\x1b[0m \x1b[2m(OK -- bound to loopback)\x1b[0m"
            );
        } else {
            println!(
                "    Admin auth      \x1b[31mdisabled\x1b[0m \x1b[2m(exposed on {}!)\x1b[0m",
                addr.ip()
            );
        }
    }

    let has_master_key = std::env::var("STACKPIT_MASTER_KEY")
        .ok()
        .filter(|k| {
            hex::decode(k.trim())
                .map(|b| b.len() == 32)
                .unwrap_or(false)
        })
        .is_some();
    if has_master_key {
        println!("    Encryption      \x1b[32menabled\x1b[0m (STACKPIT_MASTER_KEY)");
    } else {
        println!(
            "    Encryption      \x1b[33mdisabled\x1b[0m \x1b[2m(set STACKPIT_MASTER_KEY for encrypted secrets)\x1b[0m"
        );
    }

    // --- Storage ---

    println!();
    println!("\x1b[1m  Storage\x1b[0m");
    println!();
    println!("    Backend         {backend}");

    if is_postgres {
        // Mask password in URL for display
        let display_url = mask_postgres_url(&database_url);
        println!("    URL             {display_url}");
    } else {
        let db_path = &config.storage.path;
        let abs_path = std::fs::canonicalize(db_path)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| db_path.to_string());
        println!("    Path            {abs_path}");

        if let Ok(meta) = std::fs::metadata(db_path) {
            println!("    Size            {}", format_bytes(meta.len() as usize));
        } else {
            println!("    Size            \x1b[2m(file not found)\x1b[0m");
        }
        // Check for WAL file size too
        let wal_path = format!("{db_path}-wal");
        if let Ok(meta) = std::fs::metadata(&wal_path) {
            if meta.len() > 0 {
                println!("    WAL size        {}", format_bytes(meta.len() as usize));
            }
        }
    }

    if config.storage.retention_days > 0 {
        println!("    Retention       {} days", config.storage.retention_days);
    } else {
        println!("    Retention       \x1b[33mforever\x1b[0m \x1b[2m(retention_days = 0)\x1b[0m");
    }

    // --- Filter ---

    println!();
    println!("\x1b[1m  Ingestion\x1b[0m");
    println!();

    let mode_label = match config.filter.mode {
        crate::config::RegistrationMode::Open => {
            "\x1b[32mopen\x1b[0m \x1b[2m(any SDK can register projects)\x1b[0m"
        }
        crate::config::RegistrationMode::Closed => {
            "\x1b[33mclosed\x1b[0m \x1b[2m(projects must be pre-registered)\x1b[0m"
        }
    };
    println!("    Registration    {mode_label}");
    println!("    Max projects    {}", config.filter.max_projects);
    if config.filter.rate_limit > 0 {
        println!(
            "    Rate limit      {} events/min",
            config.filter.rate_limit
        );
    } else {
        println!("    Rate limit      \x1b[2munlimited\x1b[0m");
    }
    if !config.filter.excluded_environments.is_empty() {
        println!(
            "    Excluded envs   {}",
            config.filter.excluded_environments.join(", ")
        );
    }
    if !config.filter.blocked_user_agents.is_empty() {
        println!(
            "    Blocked UAs     {} pattern(s)",
            config.filter.blocked_user_agents.len()
        );
    }

    // --- Live data (requires DB connection) ---

    let pool = match db::create_pool(&database_url).await {
        Ok(p) => p,
        Err(e) => {
            println!();
            println!("\x1b[31m  Could not connect to database: {e}\x1b[0m");
            return Ok(());
        }
    };

    // Gather stats in one go
    let project_count: i64 = sqlx::query(db::sql!("SELECT COUNT(*) FROM projects"))
        .fetch_one(&pool)
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);

    let event_count: i64 = sqlx::query(db::sql!("SELECT COUNT(*) FROM events"))
        .fetch_one(&pool)
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);

    let issue_count: i64 = sqlx::query(db::sql!("SELECT COUNT(*) FROM issues"))
        .fetch_one(&pool)
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);

    let unresolved_count: i64 = sqlx::query(db::sql!(
        "SELECT COUNT(*) FROM issues WHERE status = 'unresolved'"
    ))
    .fetch_one(&pool)
    .await
    .map(|r| r.get(0))
    .unwrap_or(0);

    let integration_count: i64 = sqlx::query(db::sql!("SELECT COUNT(*) FROM integrations"))
        .fetch_one(&pool)
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);

    let alert_count: i64 = sqlx::query(db::sql!("SELECT COUNT(*) FROM alert_rules"))
        .fetch_one(&pool)
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);

    let sourcemap_count: i64 = sqlx::query(db::sql!("SELECT COUNT(*) FROM sourcemaps"))
        .fetch_one(&pool)
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);

    println!();
    println!("\x1b[1m  Data\x1b[0m");
    println!();
    println!("    Projects        {project_count}");
    println!("    Events          {event_count}");
    println!("    Issues          {issue_count} ({unresolved_count} unresolved)");
    if sourcemap_count > 0 {
        println!("    Sourcemaps      {sourcemap_count}");
    }

    // Oldest / newest event
    let oldest: Option<i64> = sqlx::query(db::sql!("SELECT MIN(received_at) FROM events"))
        .fetch_one(&pool)
        .await
        .ok()
        .and_then(|r| r.get(0));

    let newest: Option<i64> = sqlx::query(db::sql!("SELECT MAX(received_at) FROM events"))
        .fetch_one(&pool)
        .await
        .ok()
        .and_then(|r| r.get(0));

    if let (Some(old), Some(new)) = (oldest, newest) {
        let fmt = |ts: i64| {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "?".to_string())
        };
        println!("    Time range      {} .. {}", fmt(old), fmt(new));
    }

    // --- Notifications ---

    if integration_count > 0 || alert_count > 0 {
        println!();
        println!("\x1b[1m  Notifications\x1b[0m");
        println!();
        println!("    Integrations    {integration_count}");
        println!("    Alert rules     {alert_count}");
        if config.notifications.rate_limit_global > 0 {
            println!(
                "    Global limit    {}/min",
                config.notifications.rate_limit_global
            );
        }
        if config.notifications.rate_limit_per_project > 0 {
            println!(
                "    Per-project     {}/min",
                config.notifications.rate_limit_per_project
            );
        }
    }

    println!();

    Ok(())
}

fn format_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn mask_postgres_url(url: &str) -> String {
    // postgres://user:password@host:port/db -> postgres://user:***@host:port/db
    if let Some(at_pos) = url.find('@') {
        if let Some(colon_pos) = url[..at_pos].rfind(':') {
            let scheme_end = url.find("://").map(|i| i + 3).unwrap_or(0);
            if colon_pos > scheme_end {
                return format!("{}***{}", &url[..colon_pos + 1], &url[at_pos..]);
            }
        }
    }
    url.to_string()
}
