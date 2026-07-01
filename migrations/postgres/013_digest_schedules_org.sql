ALTER TABLE digest_schedules ADD COLUMN org_id BIGINT NOT NULL DEFAULT 1 REFERENCES organizations(org_id);

UPDATE digest_schedules
SET org_id = COALESCE(
    (SELECT p.org_id FROM projects p WHERE p.project_id = digest_schedules.project_id),
    1
);
