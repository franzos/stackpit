CREATE TABLE IF NOT EXISTS events (
    event_id         TEXT PRIMARY KEY,
    item_type        TEXT NOT NULL,
    payload          BYTEA NOT NULL,
    project_id       BIGINT NOT NULL,
    public_key       TEXT NOT NULL,
    timestamp        BIGINT NOT NULL,
    level            TEXT,
    platform         TEXT,
    release          TEXT,
    environment      TEXT,
    server_name      TEXT,
    transaction_name TEXT,
    title            TEXT,
    sdk_name         TEXT,
    sdk_version      TEXT,
    received_at      BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    fingerprint      TEXT,
    monitor_slug     TEXT,
    session_status   TEXT,
    parent_event_id  TEXT
);

CREATE INDEX IF NOT EXISTS idx_events_project_time ON events (project_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_events_type_time ON events (item_type, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_events_release ON events (release, timestamp DESC) WHERE release IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_events_level ON events (project_id, level, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_events_received ON events (received_at);
CREATE INDEX IF NOT EXISTS idx_events_fingerprint ON events (fingerprint, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_events_monitor ON events (project_id, monitor_slug, timestamp DESC) WHERE monitor_slug IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_events_parent ON events (parent_event_id) WHERE parent_event_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_events_project_type_time ON events (project_id, item_type, timestamp DESC);

CREATE TABLE IF NOT EXISTS logs (
    id               BIGSERIAL PRIMARY KEY,
    payload          BYTEA NOT NULL,
    project_id       BIGINT NOT NULL,
    public_key       TEXT NOT NULL,
    timestamp        BIGINT NOT NULL,
    received_at      BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    release          TEXT,
    environment      TEXT,
    trace_id         TEXT,
    span_id          TEXT,
    level            TEXT,
    body             TEXT,
    attributes       TEXT
);

CREATE INDEX IF NOT EXISTS idx_logs_project_time ON logs (project_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_logs_trace ON logs (trace_id);
CREATE INDEX IF NOT EXISTS idx_logs_level ON logs (project_id, level, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_logs_received ON logs (received_at);
CREATE INDEX IF NOT EXISTS idx_logs_span ON logs (span_id);

CREATE TABLE IF NOT EXISTS attachments (
    id           BIGSERIAL PRIMARY KEY,
    event_id     TEXT NOT NULL REFERENCES events(event_id) ON DELETE CASCADE,
    filename     TEXT NOT NULL,
    content_type TEXT,
    data         BYTEA NOT NULL,
    UNIQUE(event_id, filename)
);

CREATE TABLE IF NOT EXISTS issues (
    fingerprint     TEXT PRIMARY KEY,
    project_id      BIGINT NOT NULL,
    title           TEXT,
    level           TEXT,
    first_seen      BIGINT NOT NULL,
    last_seen       BIGINT NOT NULL,
    event_count     BIGINT NOT NULL DEFAULT 1,
    status          TEXT NOT NULL DEFAULT 'unresolved',
    item_type       TEXT NOT NULL DEFAULT 'event',
    user_hll        BYTEA,
    sentry_group_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_issues_project_time ON issues (project_id, last_seen DESC);
CREATE INDEX IF NOT EXISTS idx_issues_project_status ON issues (project_id, status, last_seen DESC);
CREATE INDEX IF NOT EXISTS idx_issues_project_type ON issues (project_id, item_type, last_seen DESC);
CREATE INDEX IF NOT EXISTS idx_issues_sentry_group_id ON issues (sentry_group_id) WHERE sentry_group_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS project_repos (
    id           BIGSERIAL PRIMARY KEY,
    project_id   BIGINT NOT NULL,
    repo_url     TEXT NOT NULL,
    forge_type   TEXT NOT NULL,
    url_template TEXT,
    UNIQUE(project_id, repo_url)
);

CREATE TABLE IF NOT EXISTS releases (
    id             BIGSERIAL PRIMARY KEY,
    project_id     BIGINT NOT NULL,
    version        TEXT NOT NULL,
    commit_sha     TEXT,
    date_released  BIGINT,
    first_event    BIGINT,
    last_event     BIGINT,
    new_groups     BIGINT NOT NULL DEFAULT 0,
    created_at     BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    UNIQUE(project_id, version)
);

CREATE TABLE IF NOT EXISTS sync_state (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

CREATE TABLE IF NOT EXISTS organizations (
    org_id     BIGSERIAL PRIMARY KEY,
    slug       TEXT NOT NULL UNIQUE,
    name       TEXT,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

INSERT INTO organizations (org_id, slug, name)
VALUES (1, 'default', 'Default')
ON CONFLICT (org_id) DO NOTHING;

CREATE TABLE IF NOT EXISTS projects (
    project_id  BIGINT PRIMARY KEY,
    name        TEXT,
    status      TEXT NOT NULL DEFAULT 'active',
    source      TEXT NOT NULL DEFAULT 'auto',
    org_id      BIGINT NOT NULL DEFAULT 1 REFERENCES organizations(org_id)
);

CREATE TABLE IF NOT EXISTS issue_tag_values (
    fingerprint  TEXT NOT NULL,
    tag_key      TEXT NOT NULL,
    tag_value    TEXT NOT NULL,
    count        BIGINT NOT NULL DEFAULT 1,
    PRIMARY KEY (fingerprint, tag_key, tag_value)
);

CREATE TABLE IF NOT EXISTS project_keys (
    public_key   TEXT PRIMARY KEY,
    project_id   BIGINT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'active',
    label        TEXT,
    created_at   BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_project_keys_project ON project_keys (project_id);

CREATE TABLE IF NOT EXISTS discarded_fingerprints (
    fingerprint  TEXT PRIMARY KEY,
    project_id   BIGINT NOT NULL,
    created_at   BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_discarded_fp_project ON discarded_fingerprints (project_id);

CREATE TABLE IF NOT EXISTS inbound_filters (
    project_id   BIGINT NOT NULL,
    filter_id    TEXT NOT NULL,
    enabled      BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (project_id, filter_id)
);

CREATE TABLE IF NOT EXISTS message_filters (
    id           BIGSERIAL PRIMARY KEY,
    project_id   BIGINT NOT NULL,
    pattern      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_message_filters_project ON message_filters (project_id);

CREATE TABLE IF NOT EXISTS rate_limits (
    id                    BIGSERIAL PRIMARY KEY,
    project_id            BIGINT NOT NULL,
    public_key            TEXT,
    max_events_per_minute INTEGER NOT NULL DEFAULT 0,
    UNIQUE(project_id, public_key)
);

CREATE TABLE IF NOT EXISTS environment_filters (
    id           BIGSERIAL PRIMARY KEY,
    project_id   BIGINT NOT NULL,
    environment  TEXT NOT NULL,
    UNIQUE(project_id, environment)
);

CREATE TABLE IF NOT EXISTS release_filters (
    id           BIGSERIAL PRIMARY KEY,
    project_id   BIGINT NOT NULL,
    pattern      TEXT NOT NULL,
    UNIQUE(project_id, pattern)
);

CREATE TABLE IF NOT EXISTS user_agent_filters (
    id           BIGSERIAL PRIMARY KEY,
    project_id   BIGINT NOT NULL,
    pattern      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ua_filters_project ON user_agent_filters (project_id);

CREATE TABLE IF NOT EXISTS filter_rules (
    id           BIGSERIAL PRIMARY KEY,
    project_id   BIGINT NOT NULL,
    field        TEXT NOT NULL,
    operator     TEXT NOT NULL,
    value        TEXT NOT NULL,
    action       TEXT NOT NULL DEFAULT 'drop',
    sample_rate  DOUBLE PRECISION,
    priority     INTEGER NOT NULL DEFAULT 0,
    enabled      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at   BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_filter_rules_project ON filter_rules (project_id, priority);

CREATE TABLE IF NOT EXISTS ip_blocklist (
    id           BIGSERIAL PRIMARY KEY,
    project_id   BIGINT NOT NULL,
    cidr         TEXT NOT NULL,
    created_at   BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    UNIQUE(project_id, cidr)
);

CREATE INDEX IF NOT EXISTS idx_ip_blocklist_project ON ip_blocklist (project_id);

CREATE TABLE IF NOT EXISTS discard_stats (
    id           BIGSERIAL PRIMARY KEY,
    project_id   BIGINT NOT NULL,
    reason       TEXT NOT NULL,
    rule_id      BIGINT,
    date         TEXT NOT NULL,
    count        BIGINT NOT NULL DEFAULT 0,
    UNIQUE(project_id, reason, rule_id, date)
);

CREATE TABLE IF NOT EXISTS integrations (
    id         BIGSERIAL PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    kind       TEXT NOT NULL,
    url        TEXT NOT NULL,
    secret     TEXT,
    encrypted  BOOLEAN NOT NULL DEFAULT FALSE,
    config     TEXT,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

CREATE TABLE IF NOT EXISTS project_integrations (
    id                 BIGSERIAL PRIMARY KEY,
    project_id         BIGINT NOT NULL,
    integration_id     BIGINT NOT NULL REFERENCES integrations(id) ON DELETE CASCADE,
    notify_new_issues  BOOLEAN NOT NULL DEFAULT TRUE,
    notify_regressions BOOLEAN NOT NULL DEFAULT TRUE,
    min_level          TEXT,
    environment_filter TEXT,
    config             TEXT,
    enabled            BOOLEAN NOT NULL DEFAULT TRUE,
    notify_threshold   BOOLEAN NOT NULL DEFAULT TRUE,
    notify_digests     BOOLEAN NOT NULL DEFAULT TRUE,
    UNIQUE(project_id, integration_id)
);

CREATE INDEX IF NOT EXISTS idx_pi_project ON project_integrations(project_id);
CREATE INDEX IF NOT EXISTS idx_pi_integration ON project_integrations(integration_id);

CREATE TABLE IF NOT EXISTS alert_rules (
    id              BIGSERIAL PRIMARY KEY,
    project_id      BIGINT,
    fingerprint     TEXT,
    trigger_kind    TEXT NOT NULL,
    threshold_count BIGINT,
    window_secs     BIGINT,
    cooldown_secs   BIGINT NOT NULL DEFAULT 3600,
    enabled         BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

CREATE TABLE IF NOT EXISTS alert_state (
    alert_rule_id   BIGINT NOT NULL,
    fingerprint     TEXT NOT NULL,
    last_triggered  BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (alert_rule_id, fingerprint)
);

CREATE TABLE IF NOT EXISTS digest_schedules (
    id              BIGSERIAL PRIMARY KEY,
    project_id      BIGINT,
    interval_secs   BIGINT NOT NULL,
    last_sent       BIGINT NOT NULL DEFAULT 0,
    enabled         BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

CREATE TABLE IF NOT EXISTS spans (
    id               BIGSERIAL PRIMARY KEY,
    span_id          TEXT NOT NULL,
    payload          BYTEA NOT NULL,
    project_id       BIGINT NOT NULL,
    public_key       TEXT NOT NULL,
    timestamp        BIGINT NOT NULL,
    received_at      BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    release          TEXT,
    environment      TEXT,
    trace_id         TEXT,
    parent_span_id   TEXT,
    op               TEXT,
    description      TEXT,
    status           TEXT,
    duration_ms      BIGINT,
    UNIQUE(span_id)
);

CREATE INDEX IF NOT EXISTS idx_spans_project_time ON spans (project_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_spans_trace ON spans (trace_id);
CREATE INDEX IF NOT EXISTS idx_spans_received ON spans (received_at);
CREATE INDEX IF NOT EXISTS idx_spans_root ON spans (trace_id, timestamp) WHERE parent_span_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_spans_project_trace ON spans (project_id, trace_id);

CREATE TABLE IF NOT EXISTS metrics (
    id               BIGSERIAL PRIMARY KEY,
    project_id       BIGINT NOT NULL,
    public_key       TEXT,
    timestamp        BIGINT NOT NULL,
    received_at      BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    mri              TEXT NOT NULL,
    metric_type      TEXT NOT NULL,
    value            DOUBLE PRECISION NOT NULL DEFAULT 0,
    "values"         TEXT,
    tags             TEXT
);

CREATE INDEX IF NOT EXISTS idx_metrics_project_time ON metrics (project_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_metrics_mri ON metrics (project_id, mri, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_metrics_received ON metrics (received_at);
CREATE INDEX IF NOT EXISTS idx_metrics_project_mri_type ON metrics (project_id, mri, metric_type);
