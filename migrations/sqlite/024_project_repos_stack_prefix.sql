-- stack_path_prefix maps a stack frame's path to the right repo when a project has several.
ALTER TABLE project_repos ADD COLUMN stack_path_prefix TEXT;
ALTER TABLE project_repos ADD COLUMN forge_type_override TEXT;
