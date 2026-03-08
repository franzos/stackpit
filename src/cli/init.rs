use anyhow::{bail, Result};
use std::path::Path;

const DEFAULT_CONFIG: &str = r#"[server]
bind = "127.0.0.1:3000"
ingest_bind = "0.0.0.0:3001"
# external_url = "http://sentry.example.com:3001"

[storage]
path = "stackpit.db"
retention_days = 90

[filter]
mode = "open"
"#;

pub fn run(path: &Path) -> Result<()> {
    if path.exists() {
        bail!("{} already exists", path.display());
    }

    std::fs::write(path, DEFAULT_CONFIG)?;
    println!("created {}", path.display());
    Ok(())
}
