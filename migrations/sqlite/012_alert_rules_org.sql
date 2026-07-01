-- SQLite: ADD COLUMN cannot carry a FK reference; enforced at query layer.
ALTER TABLE alert_rules ADD COLUMN org_id INTEGER NOT NULL DEFAULT 1;

UPDATE alert_rules
SET org_id = COALESCE(
    (SELECT p.org_id FROM projects p WHERE p.project_id = alert_rules.project_id),
    1
);
