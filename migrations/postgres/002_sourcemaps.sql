CREATE TABLE IF NOT EXISTS sourcemaps (
    id           BIGSERIAL PRIMARY KEY,
    debug_id     TEXT NOT NULL,
    source_url   TEXT,
    data         BYTEA NOT NULL,
    project_id   BIGINT NOT NULL,
    created_at   BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    UNIQUE(debug_id)
);

CREATE INDEX IF NOT EXISTS idx_sourcemaps_debug_id ON sourcemaps (debug_id);
CREATE INDEX IF NOT EXISTS idx_sourcemaps_project ON sourcemaps (project_id);

CREATE TABLE IF NOT EXISTS upload_chunks (
    checksum     TEXT PRIMARY KEY,
    data         BYTEA NOT NULL,
    created_at   BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);
