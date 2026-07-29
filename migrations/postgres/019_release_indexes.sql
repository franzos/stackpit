-- Releases are now read per (project_id, release) rather than scanned out of
-- events ad hoc. The existing idx_events_release leads with `release`, which
-- serves neither the per-project rollup nor the backfill.
CREATE INDEX IF NOT EXISTS idx_events_project_release
    ON events (project_id, release, timestamp, fingerprint)
    WHERE release IS NOT NULL;

-- UNIQUE(project_id, version) can't serve a version-only lookup.
CREATE INDEX IF NOT EXISTS idx_releases_version ON releases (version);
