-- Postgres half of the qualified external-issue key. Must land the same
-- `UNIQUE (fingerprint, integration_id, external_id)` as the SQLite file.
-- `tests/migration_parity.rs` compares table sets and filenames, not per-column
-- constraints, so this file has no automated guard: the two are diffed by hand.
--
-- The constraint name is looked up rather than assumed, following 025's idiom —
-- a wrong literal in DROP CONSTRAINT IF EXISTS would silently leave the old,
-- stricter key in place and the widening would be a no-op.
--
-- No `NULLS NOT DISTINCT`: orphaned links all carry `integration_id IS NULL`
-- and depend on staying mutually distinct. Postgres's default (NULLs distinct)
-- is what we want, and matches SQLite.
DO $$
DECLARE uq_name TEXT;
BEGIN
    SELECT c.conname INTO uq_name
    FROM pg_constraint c
    WHERE c.conrelid = 'issue_external_links'::regclass
      AND c.contype = 'u'
      AND c.conkey = ARRAY[
          (SELECT a.attnum FROM pg_attribute a
            WHERE a.attrelid = 'issue_external_links'::regclass
              AND a.attname = 'fingerprint'),
          (SELECT a.attnum FROM pg_attribute a
            WHERE a.attrelid = 'issue_external_links'::regclass
              AND a.attname = 'integration_id')
      ];

    IF uq_name IS NULL THEN
        RAISE EXCEPTION 'no (fingerprint, integration_id) unique constraint on issue_external_links to widen';
    END IF;

    EXECUTE format('ALTER TABLE issue_external_links DROP CONSTRAINT %I', uq_name);
END $$;

ALTER TABLE issue_external_links
    ADD CONSTRAINT issue_external_links_fingerprint_integration_external_key
    UNIQUE (fingerprint, integration_id, external_id);
