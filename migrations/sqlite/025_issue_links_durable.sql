-- Filed links outlive their integration: no FK, nullable integration_id, and name/kind denormalised so the row still reads after a delete.
PRAGMA defer_foreign_keys = ON;

CREATE TABLE issue_external_links_new (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id       INTEGER NOT NULL,
    fingerprint      TEXT    NOT NULL,
    integration_id   INTEGER,
    integration_name TEXT    NOT NULL,
    integration_kind TEXT    NOT NULL,
    external_id      TEXT    NOT NULL,
    external_url     TEXT    NOT NULL,
    external_state   TEXT,
    created_at       INTEGER NOT NULL,
    UNIQUE (fingerprint, integration_id)
);

INSERT INTO issue_external_links_new
    (id, project_id, fingerprint, integration_id, integration_name, integration_kind,
     external_id, external_url, external_state, created_at)
SELECT l.id, l.project_id, l.fingerprint, l.integration_id,
       COALESCE(i.name, 'unknown'), COALESCE(i.kind, 'unknown'),
       l.external_id, l.external_url, l.external_state, l.created_at
FROM issue_external_links l
LEFT JOIN integrations i ON i.id = l.integration_id;

DROP TABLE issue_external_links;
ALTER TABLE issue_external_links_new RENAME TO issue_external_links;

CREATE INDEX idx_issue_external_links_fingerprint ON issue_external_links (fingerprint);
CREATE INDEX idx_issue_external_links_integration ON issue_external_links (integration_id);
