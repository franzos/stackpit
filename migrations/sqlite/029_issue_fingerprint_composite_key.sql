-- The fingerprint is a grouping key, not a security boundary: FNV-1a over
-- attacker-controlled bytes, so a DSN holder can construct a collision with an
-- issue in another project. Keying issues, discards and tag counts by
-- `(project_id, fingerprint)` is what makes such a collision harmless — it can
-- only ever touch the sender's own project.
--
-- Nothing references these three tables by foreign key, so the rebuild needs
-- no child staging. Every index is recreated by hand: SQLite drops them with
-- the table.
PRAGMA defer_foreign_keys = ON;

CREATE TABLE issues_new (
    fingerprint     TEXT NOT NULL,
    project_id      INTEGER NOT NULL,
    title           TEXT,
    level           TEXT,
    first_seen      INTEGER NOT NULL,
    last_seen       INTEGER NOT NULL,
    event_count     INTEGER NOT NULL DEFAULT 1,
    status          TEXT NOT NULL DEFAULT 'unresolved',
    item_type       TEXT NOT NULL DEFAULT 'event',
    user_hll        BLOB,
    sentry_group_id TEXT,
    PRIMARY KEY (project_id, fingerprint)
);

INSERT INTO issues_new
    (fingerprint, project_id, title, level, first_seen, last_seen, event_count,
     status, item_type, user_hll, sentry_group_id)
SELECT fingerprint, project_id, title, level, first_seen, last_seen, event_count,
       status, item_type, user_hll, sentry_group_id
FROM issues;

DROP TABLE issues;
ALTER TABLE issues_new RENAME TO issues;

CREATE INDEX idx_issues_project_time ON issues (project_id, last_seen DESC);
CREATE INDEX idx_issues_project_status ON issues (project_id, status, last_seen DESC);
CREATE INDEX idx_issues_project_type ON issues (project_id, item_type, last_seen DESC);
CREATE INDEX idx_issues_sentry_group_id ON issues (sentry_group_id) WHERE sentry_group_id IS NOT NULL;
CREATE INDEX idx_issues_fingerprint ON issues (fingerprint);

CREATE TABLE discarded_fingerprints_new (
    fingerprint  TEXT NOT NULL,
    project_id   INTEGER NOT NULL,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (project_id, fingerprint)
);

INSERT INTO discarded_fingerprints_new (fingerprint, project_id, created_at)
SELECT fingerprint, project_id, created_at FROM discarded_fingerprints;

DROP TABLE discarded_fingerprints;
ALTER TABLE discarded_fingerprints_new RENAME TO discarded_fingerprints;

CREATE INDEX idx_discarded_fp_project ON discarded_fingerprints (project_id);

-- Tag rows had no project column at all; the inner join backfills it and drops
-- rows whose issue is already gone (they were unreachable anyway).
CREATE TABLE issue_tag_values_new (
    project_id   INTEGER NOT NULL,
    fingerprint  TEXT NOT NULL,
    tag_key      TEXT NOT NULL,
    tag_value    TEXT NOT NULL,
    count        INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (project_id, fingerprint, tag_key, tag_value)
);

INSERT INTO issue_tag_values_new (project_id, fingerprint, tag_key, tag_value, count)
SELECT i.project_id, t.fingerprint, t.tag_key, t.tag_value, t.count
FROM issue_tag_values t
JOIN issues i ON i.fingerprint = t.fingerprint;

DROP TABLE issue_tag_values;
ALTER TABLE issue_tag_values_new RENAME TO issue_tag_values;

CREATE INDEX idx_issue_tag_values_project_fp_key_count
ON issue_tag_values (project_id, fingerprint, tag_key, count DESC);
