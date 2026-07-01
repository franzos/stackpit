-- SQLite: ADD COLUMN cannot carry a FK reference; enforced at query layer.
ALTER TABLE integrations ADD COLUMN org_id INTEGER NOT NULL DEFAULT 1;
-- Global name UNIQUE kept: SQLite cannot drop an inline constraint without a table rebuild.
-- Per-org uniqueness is a known limitation until a future rebuild migration.
