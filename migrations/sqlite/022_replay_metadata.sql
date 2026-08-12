-- Per-replay display metadata, joined to `events` by event_id.
--
-- Deliberately NOT columns on `events`: replays share that table with every
-- other item type, and BULK_CHUNK_SIZE is derived from 21 bind params per event
-- against SQLite's 32766 variable limit. Six more columns would make a full
-- chunk emit 40,500 binds and fail only under load. A separate table also keeps
-- the cost off every non-replay write.
--
-- Forward-only: replays stored before this migration have no row and render
-- blanks. There is no backfill.
CREATE TABLE IF NOT EXISTS replay_metadata (
    -- CASCADE so retention's `DELETE FROM events` takes the metadata with it,
    -- the same contract `attachments` already relies on.
    event_id      TEXT PRIMARY KEY REFERENCES events(event_id) ON DELETE CASCADE,
    project_id    INTEGER NOT NULL,
    duration_ms   INTEGER,
    url           TEXT,
    user_label    TEXT,
    browser       TEXT,
    os            TEXT,
    error_count   INTEGER NOT NULL DEFAULT 0
);

-- The replay list pages by project; the join is by event_id (the primary key).
CREATE INDEX IF NOT EXISTS idx_replay_metadata_project
    ON replay_metadata (project_id);
