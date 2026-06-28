ALTER TABLE events ADD COLUMN trace_id TEXT;
ALTER TABLE events ADD COLUMN duration_ms INTEGER;
ALTER TABLE spans ADD COLUMN start_ms INTEGER;

CREATE INDEX IF NOT EXISTS idx_events_trace ON events (trace_id) WHERE trace_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_events_project_txn ON events (project_id, transaction_name, timestamp) WHERE transaction_name IS NOT NULL;

CREATE TABLE IF NOT EXISTS transaction_metrics (
    project_id       INTEGER NOT NULL,
    transaction_name TEXT NOT NULL,
    hour_bucket      INTEGER NOT NULL,
    count            INTEGER NOT NULL DEFAULT 0,
    sum_duration_ms  INTEGER NOT NULL DEFAULT 0,
    failed_count     INTEGER NOT NULL DEFAULT 0,
    bucket_0         INTEGER NOT NULL DEFAULT 0,
    bucket_1         INTEGER NOT NULL DEFAULT 0,
    bucket_2         INTEGER NOT NULL DEFAULT 0,
    bucket_3         INTEGER NOT NULL DEFAULT 0,
    bucket_4         INTEGER NOT NULL DEFAULT 0,
    bucket_5         INTEGER NOT NULL DEFAULT 0,
    bucket_6         INTEGER NOT NULL DEFAULT 0,
    bucket_7         INTEGER NOT NULL DEFAULT 0,
    bucket_8         INTEGER NOT NULL DEFAULT 0,
    bucket_9         INTEGER NOT NULL DEFAULT 0,
    bucket_10        INTEGER NOT NULL DEFAULT 0,
    bucket_11        INTEGER NOT NULL DEFAULT 0,
    bucket_12        INTEGER NOT NULL DEFAULT 0,
    bucket_13        INTEGER NOT NULL DEFAULT 0,
    bucket_14        INTEGER NOT NULL DEFAULT 0,
    bucket_15        INTEGER NOT NULL DEFAULT 0,
    bucket_16        INTEGER NOT NULL DEFAULT 0,
    bucket_17        INTEGER NOT NULL DEFAULT 0,
    bucket_18        INTEGER NOT NULL DEFAULT 0,
    bucket_19        INTEGER NOT NULL DEFAULT 0,
    bucket_20        INTEGER NOT NULL DEFAULT 0,
    bucket_21        INTEGER NOT NULL DEFAULT 0,
    bucket_22        INTEGER NOT NULL DEFAULT 0,
    bucket_23        INTEGER NOT NULL DEFAULT 0,
    users_hll        BLOB,
    first_seen       INTEGER NOT NULL,
    last_seen        INTEGER NOT NULL,
    PRIMARY KEY (project_id, transaction_name, hour_bucket)
);

CREATE INDEX IF NOT EXISTS idx_transaction_metrics_project ON transaction_metrics (project_id, hour_bucket);

DELETE FROM issues WHERE item_type = 'transaction';
