CREATE INDEX IF NOT EXISTS idx_issue_tag_values_fp_key_count
ON issue_tag_values (fingerprint, tag_key, count DESC);
