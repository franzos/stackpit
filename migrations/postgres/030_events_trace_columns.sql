-- Postgres half of the transaction stitching columns. See the SQLite file for
-- why they are columns rather than payload reads.
--
-- `start_ms` is BIGINT, not INTEGER: Postgres INTEGER is INT4 and would truncate
-- an epoch-millisecond value.
ALTER TABLE events ADD COLUMN span_id TEXT;
ALTER TABLE events ADD COLUMN parent_span_id TEXT;
ALTER TABLE events ADD COLUMN start_ms BIGINT;
