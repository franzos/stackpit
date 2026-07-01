-- SQLite: ADD COLUMN cannot carry a FK reference; enforced at query layer.
ALTER TABLE digest_schedules ADD COLUMN org_id INTEGER NOT NULL DEFAULT 1;

UPDATE digest_schedules
SET org_id = COALESCE(
    (SELECT p.org_id FROM projects p WHERE p.project_id = digest_schedules.project_id),
    1
);
