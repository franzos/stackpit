DROP TABLE IF EXISTS upload_chunks;

CREATE TABLE upload_chunks (
    checksum     TEXT NOT NULL,
    project_id   INTEGER NOT NULL,
    data         BLOB NOT NULL,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (checksum, project_id)
);
