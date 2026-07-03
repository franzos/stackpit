-- Rebuild uniqueness as (project_id, debug_id) so debug_id is namespaced per project.
ALTER TABLE sourcemaps DROP CONSTRAINT IF EXISTS sourcemaps_debug_id_key;
ALTER TABLE sourcemaps ADD CONSTRAINT sourcemaps_project_debug_id_key UNIQUE (project_id, debug_id);
