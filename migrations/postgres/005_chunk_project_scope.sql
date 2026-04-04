DROP TABLE IF EXISTS upload_chunks;

CREATE TABLE upload_chunks (
    checksum     TEXT NOT NULL,
    project_id   BIGINT NOT NULL,
    data         BYTEA NOT NULL,
    created_at   BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    PRIMARY KEY (checksum, project_id)
);
