-- Forge coordinates come from project_repos now; existing override rows are dropped, not migrated.
DROP TABLE IF EXISTS project_tracker_targets;
