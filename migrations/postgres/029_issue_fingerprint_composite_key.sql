-- Postgres half of the composite `(project_id, fingerprint)` key. The
-- fingerprint is a grouping key, not a security boundary: FNV-1a over
-- attacker-controlled bytes, so a DSN holder can construct a collision with an
-- issue in another project. The composite key is what makes such a collision
-- harmless — it can only ever touch the sender's own project.
--
-- Primary-key constraint names are looked up rather than assumed, following
-- 028's idiom: a wrong literal in DROP CONSTRAINT IF EXISTS would leave the
-- old single-column key in place and the widening would be a no-op.
DO $$
DECLARE pk_name TEXT;
BEGIN
    SELECT conname INTO pk_name FROM pg_constraint
    WHERE conrelid = 'issues'::regclass AND contype = 'p';
    IF pk_name IS NULL THEN
        RAISE EXCEPTION 'no primary key on issues to widen';
    END IF;
    EXECUTE format('ALTER TABLE issues DROP CONSTRAINT %I', pk_name);
END $$;

ALTER TABLE issues ADD CONSTRAINT issues_pkey PRIMARY KEY (project_id, fingerprint);
CREATE INDEX IF NOT EXISTS idx_issues_fingerprint ON issues (fingerprint);

DO $$
DECLARE pk_name TEXT;
BEGIN
    SELECT conname INTO pk_name FROM pg_constraint
    WHERE conrelid = 'discarded_fingerprints'::regclass AND contype = 'p';
    IF pk_name IS NULL THEN
        RAISE EXCEPTION 'no primary key on discarded_fingerprints to widen';
    END IF;
    EXECUTE format('ALTER TABLE discarded_fingerprints DROP CONSTRAINT %I', pk_name);
END $$;

ALTER TABLE discarded_fingerprints
    ADD CONSTRAINT discarded_fingerprints_pkey PRIMARY KEY (project_id, fingerprint);

-- Tag rows had no project column at all; the backfill joins through issues and
-- drops rows whose issue is already gone (they were unreachable anyway).
ALTER TABLE issue_tag_values ADD COLUMN project_id BIGINT;
UPDATE issue_tag_values t SET project_id = i.project_id
FROM issues i WHERE i.fingerprint = t.fingerprint;
DELETE FROM issue_tag_values WHERE project_id IS NULL;
ALTER TABLE issue_tag_values ALTER COLUMN project_id SET NOT NULL;

DO $$
DECLARE pk_name TEXT;
BEGIN
    SELECT conname INTO pk_name FROM pg_constraint
    WHERE conrelid = 'issue_tag_values'::regclass AND contype = 'p';
    IF pk_name IS NULL THEN
        RAISE EXCEPTION 'no primary key on issue_tag_values to widen';
    END IF;
    EXECUTE format('ALTER TABLE issue_tag_values DROP CONSTRAINT %I', pk_name);
END $$;

ALTER TABLE issue_tag_values
    ADD CONSTRAINT issue_tag_values_pkey PRIMARY KEY (project_id, fingerprint, tag_key, tag_value);

DROP INDEX IF EXISTS idx_issue_tag_values_fp_key_count;
CREATE INDEX IF NOT EXISTS idx_issue_tag_values_project_fp_key_count
ON issue_tag_values (project_id, fingerprint, tag_key, count DESC);
