ALTER TABLE integrations ADD COLUMN org_id BIGINT NOT NULL DEFAULT 1 REFERENCES organizations(org_id);
-- Downgrade global name unique to per-org unique.
ALTER TABLE integrations DROP CONSTRAINT integrations_name_key;
CREATE UNIQUE INDEX ON integrations(org_id, name);
