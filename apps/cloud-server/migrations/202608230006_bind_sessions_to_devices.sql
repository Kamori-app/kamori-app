ALTER TABLE refresh_tokens
    ADD COLUMN device_id UUID REFERENCES devices(id) ON DELETE SET NULL;

CREATE INDEX idx_refresh_tokens_device_active
    ON refresh_tokens (device_id, user_id)
    WHERE revoked_at IS NULL;
