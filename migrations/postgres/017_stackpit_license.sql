CREATE TABLE stackpit_license (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    blob         TEXT    NOT NULL,
    license_id   TEXT    NOT NULL,
    customer     TEXT    NOT NULL,
    email        TEXT    NOT NULL,
    product      TEXT    NOT NULL DEFAULT '',
    tier         TEXT    NOT NULL DEFAULT '',
    issued_at    TEXT    NOT NULL,
    expires_at   TEXT,
    features     TEXT    NOT NULL DEFAULT '[]',
    max_orgs     INTEGER,
    activated_at TEXT    NOT NULL,
    verified_at  TEXT    NOT NULL
);
