CREATE TABLE IF NOT EXISTS session_aggregates (
    project_id        BIGINT NOT NULL,
    release           TEXT NOT NULL DEFAULT '',
    environment       TEXT NOT NULL DEFAULT '',
    day_bucket        BIGINT NOT NULL DEFAULT 0,
    sessions_total    BIGINT NOT NULL DEFAULT 0,
    sessions_crashed  BIGINT NOT NULL DEFAULT 0,
    sessions_errored  BIGINT NOT NULL DEFAULT 0,
    sessions_abnormal BIGINT NOT NULL DEFAULT 0,
    users_hll         BYTEA,
    users_crashed_hll BYTEA,
    has_aggregate     BIGINT NOT NULL DEFAULT 0,
    first_seen        BIGINT NOT NULL,
    last_seen         BIGINT NOT NULL,
    PRIMARY KEY (project_id, release, environment, day_bucket)
);

CREATE INDEX IF NOT EXISTS idx_session_aggregates_project ON session_aggregates (project_id);
CREATE INDEX IF NOT EXISTS idx_session_aggregates_project_day ON session_aggregates (project_id, day_bucket);
