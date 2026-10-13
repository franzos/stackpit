-- Per-project retention overrides. A row may only shorten the server default
-- for its category; the application enforces that bound, not the schema.
CREATE TABLE project_retention (
    project_id BIGINT NOT NULL,
    category   TEXT   NOT NULL,
    days       BIGINT NOT NULL,
    PRIMARY KEY (project_id, category),
    CHECK (days > 0)
);
