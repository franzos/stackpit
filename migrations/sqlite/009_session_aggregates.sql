CREATE TABLE IF NOT EXISTS session_aggregates (
    project_id        INTEGER NOT NULL,
    release           TEXT NOT NULL DEFAULT '',
    environment       TEXT NOT NULL DEFAULT '',
    day_bucket        INTEGER NOT NULL DEFAULT 0,
    sessions_total    INTEGER NOT NULL DEFAULT 0,
    sessions_crashed  INTEGER NOT NULL DEFAULT 0,
    sessions_errored  INTEGER NOT NULL DEFAULT 0,
    sessions_abnormal INTEGER NOT NULL DEFAULT 0,
    users_hll         BLOB,
    users_crashed_hll BLOB,
    has_aggregate     INTEGER NOT NULL DEFAULT 0,
    first_seen        INTEGER NOT NULL,
    last_seen         INTEGER NOT NULL,
    PRIMARY KEY (project_id, release, environment, day_bucket)
);

CREATE INDEX IF NOT EXISTS idx_session_aggregates_project ON session_aggregates (project_id);
CREATE INDEX IF NOT EXISTS idx_session_aggregates_project_day ON session_aggregates (project_id, day_bucket);
