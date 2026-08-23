CREATE TABLE account_recovery_attempts (
    token_hash BYTEA PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    request_hash BYTEA CHECK (
        request_hash IS NULL OR octet_length(request_hash) = 32
    ),
    expires_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_account_recovery_attempts_expiry
    ON account_recovery_attempts (expires_at)
    WHERE completed_at IS NULL;

CREATE INDEX idx_account_recovery_attempts_completed
    ON account_recovery_attempts (completed_at)
    WHERE completed_at IS NOT NULL;
