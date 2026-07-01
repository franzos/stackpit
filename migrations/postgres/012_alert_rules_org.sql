ALTER TABLE alert_rules ADD COLUMN org_id BIGINT NOT NULL DEFAULT 1 REFERENCES organizations(org_id);

UPDATE alert_rules
SET org_id = COALESCE(
    (SELECT p.org_id FROM projects p WHERE p.project_id = alert_rules.project_id),
    1
);
