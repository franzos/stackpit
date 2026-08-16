-- SQLite can't drop the inline UNIQUE(name): recreate the table, and stage the children because DROP TABLE fires their ON DELETE CASCADE.
PRAGMA defer_foreign_keys = ON;

CREATE TABLE _mig023_project_integrations AS SELECT * FROM project_integrations;
CREATE TABLE _mig023_issue_external_links AS SELECT * FROM issue_external_links;
CREATE TABLE _mig023_project_tracker_targets AS SELECT * FROM project_tracker_targets;

CREATE TABLE integrations_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    org_id     INTEGER NOT NULL DEFAULT 1 REFERENCES organizations (org_id),
    name       TEXT    NOT NULL,
    kind       TEXT    NOT NULL,
    url        TEXT,
    secret     TEXT,
    encrypted  INTEGER NOT NULL DEFAULT 0,
    config     TEXT,
    is_global  INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (org_id, name)
);

INSERT INTO integrations_new (id, org_id, name, kind, url, secret, encrypted, config, created_at)
SELECT id, org_id, name, kind, url, secret, encrypted, config, created_at FROM integrations;

DROP TABLE integrations;
ALTER TABLE integrations_new RENAME TO integrations;

-- DELETE first in case the install has foreign_keys off and nothing cascaded.
DELETE FROM project_integrations;
INSERT INTO project_integrations SELECT * FROM _mig023_project_integrations;
DELETE FROM issue_external_links;
INSERT INTO issue_external_links SELECT * FROM _mig023_issue_external_links;
DELETE FROM project_tracker_targets;
INSERT INTO project_tracker_targets SELECT * FROM _mig023_project_tracker_targets;

DROP TABLE _mig023_project_integrations;
DROP TABLE _mig023_issue_external_links;
DROP TABLE _mig023_project_tracker_targets;
