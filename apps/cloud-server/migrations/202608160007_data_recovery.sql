-- Development migration for databases created before the recovery-kit verifier
-- became part of the unreleased MVP schema. Compatibility is intentionally not
-- provided: a database containing pre-MVP users must be recreated.
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS recovery_verifier_hash BYTEA;

ALTER TABLE users
    DROP CONSTRAINT IF EXISTS users_recovery_verifier_hash_check;

ALTER TABLE users
    ADD CONSTRAINT users_recovery_verifier_hash_check CHECK (
        octet_length(recovery_verifier_hash) = 32
    );

ALTER TABLE users
    ALTER COLUMN recovery_verifier_hash SET NOT NULL;

CREATE TABLE IF NOT EXISTS security_space_recovery_keys (
    space_id UUID NOT NULL REFERENCES security_spaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key_epoch INTEGER NOT NULL CHECK (key_epoch > 0),
    encrypted_key_package BYTEA NOT NULL CHECK (
        octet_length(encrypted_key_package) BETWEEN 49 AND 65536
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (space_id, user_id, key_epoch),
    FOREIGN KEY (space_id, user_id)
        REFERENCES security_space_members(space_id, user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_security_space_recovery_keys_user
    ON security_space_recovery_keys (user_id, space_id, key_epoch DESC);
