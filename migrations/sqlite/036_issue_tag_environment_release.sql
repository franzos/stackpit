-- environment and release are issue tags derived from the event columns; SDK-sent tags with these keys are replaced.
DELETE FROM issue_tag_values WHERE tag_key IN ('environment', 'release');

INSERT INTO issue_tag_values (project_id, fingerprint, tag_key, tag_value, count)
SELECT project_id, fingerprint, 'environment', environment, COUNT(*)
FROM events
WHERE fingerprint IS NOT NULL AND environment IS NOT NULL AND environment <> ''
GROUP BY project_id, fingerprint, environment;

INSERT INTO issue_tag_values (project_id, fingerprint, tag_key, tag_value, count)
SELECT project_id, fingerprint, 'release', release, COUNT(*)
FROM events
WHERE fingerprint IS NOT NULL AND release IS NOT NULL AND release <> ''
GROUP BY project_id, fingerprint, release;

CREATE INDEX idx_itv_key_value ON issue_tag_values (project_id, tag_key, tag_value, fingerprint);
