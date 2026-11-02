-- A project's platform, set on creation or by the first event that carries one.
-- Backfill: manual projects take the label of their first key, which is where
-- the new-project form stored the platform; the rest take the most recent
-- platform among their newest 200 events. `platform` is in no index, so the
-- bound keeps a project without any platform from fetching every one of its rows.
ALTER TABLE projects ADD COLUMN platform TEXT;

UPDATE projects
SET platform = NULLIF((
    SELECT label FROM project_keys k
    WHERE k.project_id = projects.project_id
    ORDER BY k.created_at, k.public_key
    LIMIT 1
), '')
WHERE source = 'manual';

UPDATE projects
SET platform = (
    SELECT platform FROM (
        SELECT platform, timestamp FROM events
        WHERE project_id = projects.project_id
        ORDER BY timestamp DESC
        LIMIT 200
    ) recent
    WHERE platform IS NOT NULL AND platform <> ''
    ORDER BY timestamp DESC
    LIMIT 1
)
WHERE platform IS NULL;
