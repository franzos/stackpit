-- Dedupe user-agent filters like the sibling filter tables. Build fails if
-- duplicate (project_id, pattern) rows already exist -- none expected, the
-- insert path simply never enforced uniqueness before.
CREATE UNIQUE INDEX IF NOT EXISTS idx_ua_filters_unique
    ON user_agent_filters (project_id, pattern);
