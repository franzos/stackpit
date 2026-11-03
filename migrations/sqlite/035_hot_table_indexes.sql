CREATE INDEX idx_events_project_time ON events (project_id, timestamp DESC, item_type);
CREATE INDEX idx_events_project_type_time ON events (project_id, item_type, timestamp DESC);
CREATE INDEX idx_events_type_time ON events (item_type, timestamp DESC);
CREATE INDEX idx_events_level ON events (project_id, level, timestamp DESC);
CREATE INDEX idx_events_received ON events (received_at);
CREATE INDEX idx_events_fingerprint ON events (fingerprint, timestamp DESC);
CREATE INDEX idx_events_monitor ON events (project_id, monitor_slug, timestamp DESC) WHERE monitor_slug IS NOT NULL;
CREATE INDEX idx_events_parent ON events (parent_event_id) WHERE parent_event_id IS NOT NULL;
CREATE INDEX idx_events_trace ON events (trace_id) WHERE trace_id IS NOT NULL;
CREATE INDEX idx_events_project_txn ON events (project_id, transaction_name, timestamp) WHERE transaction_name IS NOT NULL;
CREATE INDEX idx_events_project_release ON events (project_id, release, timestamp, fingerprint) WHERE release IS NOT NULL;
CREATE INDEX idx_events_trace_list ON events (item_type, timestamp DESC, project_id, trace_id, start_ms, duration_ms, parent_span_id, transaction_name) WHERE trace_id IS NOT NULL;
CREATE INDEX idx_events_txn_duration ON events (project_id, transaction_name, duration_ms DESC, event_id, timestamp) WHERE item_type = 'transaction';

CREATE INDEX idx_spans_project_time ON spans (project_id, timestamp DESC);
CREATE INDEX idx_spans_trace ON spans (trace_id);
CREATE INDEX idx_spans_received ON spans (received_at);
CREATE INDEX idx_spans_root ON spans (trace_id, timestamp) WHERE parent_span_id IS NULL;
CREATE INDEX idx_spans_project_trace ON spans (project_id, trace_id);
