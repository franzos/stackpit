-- Rebuild uniqueness as (project_id, debug_id) so debug_id is namespaced per project.
-- SQLite can't drop a table-level UNIQUE constraint, so recreate and copy rows.
CREATE TABLE sourcemaps_new (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    debug_id     TEXT NOT NULL,
    source_url   TEXT,
    data         BLOB NOT NULL,
    project_id   INTEGER NOT NULL,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(project_id, debug_id)
);

INSERT INTO sourcemaps_new (id, debug_id, source_url, data, project_id, created_at)
SELECT id, debug_id, source_url, data, project_id, created_at FROM sourcemaps;

DROP TABLE sourcemaps;
ALTER TABLE sourcemaps_new RENAME TO sourcemaps;

CREATE INDEX IF NOT EXISTS idx_sourcemaps_debug_id ON sourcemaps (debug_id);
CREATE INDEX IF NOT EXISTS idx_sourcemaps_project ON sourcemaps (project_id);
