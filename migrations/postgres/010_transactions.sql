ALTER TABLE events ADD COLUMN trace_id TEXT;
ALTER TABLE events ADD COLUMN duration_ms BIGINT;
ALTER TABLE spans ADD COLUMN start_ms BIGINT;

CREATE INDEX IF NOT EXISTS idx_events_trace ON events (trace_id) WHERE trace_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_events_project_txn ON events (project_id, transaction_name, timestamp) WHERE transaction_name IS NOT NULL;

CREATE TABLE IF NOT EXISTS transaction_metrics (
    project_id       BIGINT NOT NULL,
    transaction_name TEXT NOT NULL,
    hour_bucket      BIGINT NOT NULL,
    count            BIGINT NOT NULL DEFAULT 0,
    sum_duration_ms  BIGINT NOT NULL DEFAULT 0,
    failed_count     BIGINT NOT NULL DEFAULT 0,
    bucket_0         BIGINT NOT NULL DEFAULT 0,
    bucket_1         BIGINT NOT NULL DEFAULT 0,
    bucket_2         BIGINT NOT NULL DEFAULT 0,
    bucket_3         BIGINT NOT NULL DEFAULT 0,
    bucket_4         BIGINT NOT NULL DEFAULT 0,
    bucket_5         BIGINT NOT NULL DEFAULT 0,
    bucket_6         BIGINT NOT NULL DEFAULT 0,
    bucket_7         BIGINT NOT NULL DEFAULT 0,
    bucket_8         BIGINT NOT NULL DEFAULT 0,
    bucket_9         BIGINT NOT NULL DEFAULT 0,
    bucket_10        BIGINT NOT NULL DEFAULT 0,
    bucket_11        BIGINT NOT NULL DEFAULT 0,
    bucket_12        BIGINT NOT NULL DEFAULT 0,
    bucket_13        BIGINT NOT NULL DEFAULT 0,
    bucket_14        BIGINT NOT NULL DEFAULT 0,
    bucket_15        BIGINT NOT NULL DEFAULT 0,
    bucket_16        BIGINT NOT NULL DEFAULT 0,
    bucket_17        BIGINT NOT NULL DEFAULT 0,
    bucket_18        BIGINT NOT NULL DEFAULT 0,
    bucket_19        BIGINT NOT NULL DEFAULT 0,
    bucket_20        BIGINT NOT NULL DEFAULT 0,
    bucket_21        BIGINT NOT NULL DEFAULT 0,
    bucket_22        BIGINT NOT NULL DEFAULT 0,
    bucket_23        BIGINT NOT NULL DEFAULT 0,
    users_hll        BYTEA,
    first_seen       BIGINT NOT NULL,
    last_seen        BIGINT NOT NULL,
    PRIMARY KEY (project_id, transaction_name, hour_bucket)
);

CREATE INDEX IF NOT EXISTS idx_transaction_metrics_project ON transaction_metrics (project_id, hour_bucket);

DELETE FROM issues WHERE item_type = 'transaction';
