CREATE TABLE IF NOT EXISTS event_payloads (
    event_id TEXT PRIMARY KEY REFERENCES events(event_id) ON DELETE CASCADE,
    payload  BYTEA NOT NULL
);

INSERT INTO event_payloads (event_id, payload)
SELECT event_id, payload FROM events;

ALTER TABLE events
    DROP COLUMN payload,
    ADD COLUMN trace_op TEXT,
    ADD COLUMN trace_status TEXT;

ALTER TABLE spans DROP COLUMN payload;

CREATE TABLE IF NOT EXISTS client_report_outcomes (
    project_id   BIGINT NOT NULL,
    hour_bucket  BIGINT NOT NULL,
    category     TEXT NOT NULL,
    reason       TEXT NOT NULL,
    quantity     BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (project_id, hour_bucket, category, reason)
);
