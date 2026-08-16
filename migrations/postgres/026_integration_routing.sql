-- Opt-out only: a global integration covers every project in its org except the ones listed here.
CREATE TABLE integration_exclusions (
    id             BIGSERIAL PRIMARY KEY,
    org_id         BIGINT NOT NULL REFERENCES organizations (org_id),
    integration_id BIGINT NOT NULL REFERENCES integrations (id) ON DELETE CASCADE,
    project_id     BIGINT NOT NULL,
    created_at     BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    UNIQUE (integration_id, project_id)
);

CREATE INDEX idx_integration_exclusions_project ON integration_exclusions (project_id);
CREATE INDEX idx_integration_exclusions_org ON integration_exclusions (org_id);

-- created_at bounds the 24h retry window; updated_at bounds failed-row retention.
CREATE TABLE notification_delivery_queue (
    id              BIGSERIAL PRIMARY KEY,
    org_id          BIGINT NOT NULL,
    project_id      BIGINT NOT NULL,
    integration_id  BIGINT NOT NULL REFERENCES integrations (id) ON DELETE CASCADE,
    payload         TEXT   NOT NULL,
    status          TEXT   NOT NULL DEFAULT 'pending',
    attempts        BIGINT NOT NULL DEFAULT 0,
    last_error      TEXT,
    next_attempt_at BIGINT NOT NULL,
    created_at      BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    updated_at      BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

CREATE INDEX idx_ndq_due ON notification_delivery_queue (status, next_attempt_at);
CREATE INDEX idx_ndq_integration ON notification_delivery_queue (integration_id, id);
CREATE INDEX idx_ndq_org ON notification_delivery_queue (org_id, status);
CREATE INDEX idx_ndq_project ON notification_delivery_queue (project_id);
