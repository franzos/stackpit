-- Natural-order sort key for release versions. Version ordering can't be done
-- in SQL (string order puts 1.0.9 above 1.0.12), and the release list is
-- paginated, so sorting in the handler would only order within a page. The key
-- is computed in Rust on write and backfilled once at startup for existing rows.
ALTER TABLE releases ADD COLUMN version_sort TEXT;

CREATE INDEX IF NOT EXISTS idx_releases_project_version_sort
    ON releases (project_id, version_sort);
