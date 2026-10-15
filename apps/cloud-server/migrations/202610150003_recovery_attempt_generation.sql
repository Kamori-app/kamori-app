-- In-flight recovery attempts from before this migration have no credential
-- binding. Invalidate them while preserving completed exact-request retries.
DELETE FROM account_recovery_attempts WHERE completed_at IS NULL;
ALTER TABLE account_recovery_attempts
    ADD COLUMN auth_generation BIGINT NOT NULL DEFAULT 0;
