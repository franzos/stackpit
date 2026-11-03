-- Needs foreign_keys off (`stackpit migrate`): with them on, DROP TABLE events cascades into its children.

CREATE TABLE events_new (
    event_id         TEXT PRIMARY KEY,
    project_id       INTEGER NOT NULL,
    item_type        TEXT NOT NULL,
    timestamp        INTEGER NOT NULL,
    received_at      INTEGER NOT NULL DEFAULT (unixepoch()),
    trace_id         TEXT,
    duration_ms      INTEGER,
    start_ms         INTEGER,
    span_id          TEXT,
    parent_span_id   TEXT,
    trace_op         TEXT,
    trace_status     TEXT,
    level            TEXT,
    fingerprint      TEXT,
    release          TEXT,
    environment      TEXT,
    transaction_name TEXT,
    title            TEXT,
    monitor_slug     TEXT,
    session_status   TEXT,
    parent_event_id  TEXT,
    platform         TEXT,
    server_name      TEXT,
    sdk_name         TEXT,
    sdk_version      TEXT,
    public_key       TEXT NOT NULL
);

INSERT INTO events_new
    (event_id, project_id, item_type, timestamp, received_at, trace_id, duration_ms,
     start_ms, span_id, parent_span_id, level, fingerprint, release, environment,
     transaction_name, title, monitor_slug, session_status, parent_event_id, platform,
     server_name, sdk_name, sdk_version, public_key)
SELECT event_id, project_id, item_type, timestamp, received_at, trace_id, duration_ms,
       start_ms, span_id, parent_span_id, level, fingerprint, release, environment,
       transaction_name, title, monitor_slug, session_status, parent_event_id, platform,
       server_name, sdk_name, sdk_version, public_key
FROM events;

CREATE TABLE event_payloads (
    event_id TEXT PRIMARY KEY REFERENCES events(event_id) ON DELETE CASCADE,
    payload  BLOB NOT NULL
);

INSERT INTO event_payloads (event_id, payload)
SELECT event_id, payload FROM events;

-- Drop before rename: renaming the old table first would repoint the children's foreign keys at it.
DROP TABLE events;
ALTER TABLE events_new RENAME TO events;

CREATE TABLE spans_new (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    span_id          TEXT NOT NULL,
    project_id       INTEGER NOT NULL,
    public_key       TEXT NOT NULL,
    timestamp        INTEGER NOT NULL,
    received_at      INTEGER NOT NULL DEFAULT (unixepoch()),
    release          TEXT,
    environment      TEXT,
    trace_id         TEXT,
    parent_span_id   TEXT,
    op               TEXT,
    description      TEXT,
    status           TEXT,
    duration_ms      INTEGER,
    start_ms         INTEGER,
    UNIQUE(span_id)
);

INSERT INTO spans_new
    (id, span_id, project_id, public_key, timestamp, received_at, release, environment,
     trace_id, parent_span_id, op, description, status, duration_ms, start_ms)
SELECT id, span_id, project_id, public_key, timestamp, received_at, release, environment,
       trace_id, parent_span_id, op, description, status, duration_ms, start_ms
FROM spans;

-- Keep the old high-water mark, which can sit above MAX(id).
DELETE FROM sqlite_sequence WHERE name = 'spans_new';
INSERT INTO sqlite_sequence (name, seq)
SELECT 'spans_new', seq FROM sqlite_sequence WHERE name = 'spans';

DROP TABLE spans;
ALTER TABLE spans_new RENAME TO spans;

CREATE TABLE client_report_outcomes (
    project_id   INTEGER NOT NULL,
    hour_bucket  INTEGER NOT NULL,
    category     TEXT NOT NULL,
    reason       TEXT NOT NULL,
    quantity     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (project_id, hour_bucket, category, reason)
);
