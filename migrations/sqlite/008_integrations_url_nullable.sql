PRAGMA defer_foreign_keys = ON;

CREATE TABLE integrations_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL UNIQUE,
    kind       TEXT NOT NULL,
    url        TEXT,
    secret     TEXT,
    encrypted  INTEGER NOT NULL DEFAULT 0,
    config     TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

INSERT INTO integrations_new (id, name, kind, url, secret, encrypted, config, created_at)
SELECT id, name, kind, url, secret, encrypted, config, created_at FROM integrations;

DROP TABLE integrations;
ALTER TABLE integrations_new RENAME TO integrations;

UPDATE integrations SET url = NULL WHERE kind = 'email';
