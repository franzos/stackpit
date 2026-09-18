-- A transaction's own span id, its parent span id and its absolute start are
-- what stitch a trace together across projects: the parent is an `http.client`
-- span in the calling app, which lives in a different project's rows. All three
-- sit in the zstd payload under `contexts.trace`, so a cross-project waterfall
-- would have to decompress every transaction to draw one row. Sentry keeps them
-- as columns on the transaction; so does this.
--
-- No index: every read reaches these through the existing `trace_id` index.
-- No backfill: rows ingested before this migration keep NULL and render the way
-- they render today, and traces expire under retention within one window.
ALTER TABLE events ADD COLUMN span_id TEXT;
ALTER TABLE events ADD COLUMN parent_span_id TEXT;
ALTER TABLE events ADD COLUMN start_ms INTEGER;
