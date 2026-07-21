CREATE TABLE issue_external_links (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id     INTEGER NOT NULL,
    fingerprint    TEXT    NOT NULL,
    integration_id INTEGER NOT NULL,
    external_id    TEXT    NOT NULL,
    external_url   TEXT    NOT NULL,
    external_state TEXT,
    created_at     INTEGER NOT NULL,
    UNIQUE (fingerprint, integration_id),
    FOREIGN KEY (integration_id) REFERENCES integrations (id) ON DELETE CASCADE
);
CREATE INDEX idx_issue_external_links_fingerprint ON issue_external_links (fingerprint);

CREATE TABLE project_tracker_targets (
    project_id     INTEGER NOT NULL,
    integration_id INTEGER NOT NULL,
    target         TEXT    NOT NULL,
    PRIMARY KEY (project_id, integration_id),
    FOREIGN KEY (integration_id) REFERENCES integrations (id) ON DELETE CASCADE
);
