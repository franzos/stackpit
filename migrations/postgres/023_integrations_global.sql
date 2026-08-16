-- Postgres got UNIQUE(org_id, name) and the org_id FK back in 014; only the column is new.
ALTER TABLE integrations ADD COLUMN is_global BOOLEAN NOT NULL DEFAULT FALSE;
