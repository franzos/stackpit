DROP INDEX IF EXISTS idx_events_release;
DROP INDEX IF EXISTS idx_events_project_time;
CREATE INDEX idx_events_project_time ON events (project_id, timestamp DESC, item_type);

CREATE INDEX IF NOT EXISTS idx_events_trace_list ON events (item_type, timestamp DESC, project_id, trace_id, start_ms, duration_ms, parent_span_id, transaction_name) WHERE trace_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_events_txn_duration ON events (project_id, transaction_name, duration_ms DESC NULLS LAST, event_id, timestamp) WHERE item_type = 'transaction';
