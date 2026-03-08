CREATE TABLE IF NOT EXISTS sourcemaps (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    debug_id     TEXT NOT NULL,
    source_url   TEXT,
    data         BLOB NOT NULL,
    project_id   INTEGER NOT NULL,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(debug_id)
);

CREATE INDEX IF NOT EXISTS idx_sourcemaps_debug_id ON sourcemaps (debug_id);
CREATE INDEX IF NOT EXISTS idx_sourcemaps_project ON sourcemaps (project_id);

CREATE TABLE IF NOT EXISTS upload_chunks (
    checksum     TEXT PRIMARY KEY,
    data         BLOB NOT NULL,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch())
);
