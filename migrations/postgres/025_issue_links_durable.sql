-- Filed links outlive their integration. Look the FK name up: a wrong literal in DROP CONSTRAINT IF EXISTS would silently leave the cascade in place.
DO $$
DECLARE fk_name TEXT;
BEGIN
    SELECT c.conname INTO fk_name
    FROM pg_constraint c
    WHERE c.conrelid = 'issue_external_links'::regclass
      AND c.contype = 'f'
      AND c.conkey = ARRAY[(
          SELECT a.attnum FROM pg_attribute a
          WHERE a.attrelid = 'issue_external_links'::regclass
            AND a.attname = 'integration_id'
      )];

    IF fk_name IS NULL THEN
        RAISE EXCEPTION 'no foreign key on issue_external_links.integration_id to drop';
    END IF;

    EXECUTE format('ALTER TABLE issue_external_links DROP CONSTRAINT %I', fk_name);
END $$;

ALTER TABLE issue_external_links ALTER COLUMN integration_id DROP NOT NULL;

ALTER TABLE issue_external_links ADD COLUMN integration_name TEXT;
ALTER TABLE issue_external_links ADD COLUMN integration_kind TEXT;

UPDATE issue_external_links l
   SET integration_name = COALESCE(
           (SELECT i.name FROM integrations i WHERE i.id = l.integration_id), 'unknown'),
       integration_kind = COALESCE(
           (SELECT i.kind FROM integrations i WHERE i.id = l.integration_id), 'unknown');

ALTER TABLE issue_external_links ALTER COLUMN integration_name SET NOT NULL;
ALTER TABLE issue_external_links ALTER COLUMN integration_kind SET NOT NULL;

CREATE INDEX idx_issue_external_links_integration ON issue_external_links (integration_id);
