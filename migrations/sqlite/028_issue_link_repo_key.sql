-- One issue may be filed into several repositories of the same integration, so
-- the key carries the external issue's own identity (owner/repo#42) rather than
-- just the integration. Sentry keys on `(organization, integration_id, key)` for
-- the same reason. Existing rows keep their bare `external_id`: the new
-- constraint is strictly weaker, so no row can violate it.
--
-- Four things this rebuild must preserve, each of which would silently regress
-- a shipped behaviour:
--   * `integration_id` stays nullable and FK-free. Link durability rests on two
--     explicit `UPDATE ... SET integration_id = NULL` statements, not a cascade;
--     re-adding the FK would delete filed links when an integration goes.
--   * both indexes are recreated by hand — SQLite drops them with the table.
--   * `id` values are preserved: the unlink route takes `link_id` from the page.
--   * no `NULLS NOT DISTINCT`. Orphaned links all carry `integration_id IS NULL`
--     and depend on staying mutually distinct.
--
-- No child staging is needed: nothing references this table since 027 dropped
-- `project_tracker_targets`.
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
    UNIQUE (fingerprint, integration_id, external_id)
);

INSERT INTO issue_external_links_new
    (id, project_id, fingerprint, integration_id, integration_name, integration_kind,
     external_id, external_url, external_state, created_at)
SELECT id, project_id, fingerprint, integration_id, integration_name, integration_kind,
       external_id, external_url, external_state, created_at
FROM issue_external_links;

DROP TABLE issue_external_links;
ALTER TABLE issue_external_links_new RENAME TO issue_external_links;

CREATE INDEX idx_issue_external_links_fingerprint ON issue_external_links (fingerprint);
CREATE INDEX idx_issue_external_links_integration ON issue_external_links (integration_id);
