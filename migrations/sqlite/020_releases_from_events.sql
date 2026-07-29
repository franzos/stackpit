-- Backfill releases that were only ever seen on events. Before this, a version
-- became a row in `releases` only via sync or a sourcemap upload, so the release
-- views had to fall back to scanning `events`.
INSERT INTO releases (project_id, version, first_event, last_event)
SELECT project_id, release, MIN(timestamp), MAX(timestamp)
FROM events
WHERE release IS NOT NULL AND release <> '' AND length(release) <= 200
GROUP BY project_id, release
ON CONFLICT(project_id, version) DO UPDATE SET
    first_event = MIN(COALESCE(releases.first_event, excluded.first_event), excluded.first_event),
    last_event  = MAX(COALESCE(releases.last_event, excluded.last_event), excluded.last_event);
