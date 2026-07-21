CREATE TABLE issue_external_links (
    id             BIGSERIAL PRIMARY KEY,
    project_id     BIGINT NOT NULL,
    fingerprint    TEXT   NOT NULL,
    integration_id BIGINT NOT NULL,
    external_id    TEXT   NOT NULL,
    external_url   TEXT   NOT NULL,
    external_state TEXT,
    created_at     BIGINT NOT NULL,
    UNIQUE (fingerprint, integration_id),
    FOREIGN KEY (integration_id) REFERENCES integrations (id) ON DELETE CASCADE
);
CREATE INDEX idx_issue_external_links_fingerprint ON issue_external_links (fingerprint);

CREATE TABLE project_tracker_targets (
    project_id     BIGINT NOT NULL,
    integration_id BIGINT NOT NULL,
    target         TEXT   NOT NULL,
    PRIMARY KEY (project_id, integration_id),
    FOREIGN KEY (integration_id) REFERENCES integrations (id) ON DELETE CASCADE
);
