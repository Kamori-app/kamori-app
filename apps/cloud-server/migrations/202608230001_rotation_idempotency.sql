ALTER TABLE security_space_epochs
    ADD COLUMN target_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN request_hash BYTEA;

ALTER TABLE security_space_epochs
    ADD CONSTRAINT security_space_epochs_request_hash_size
        CHECK (request_hash IS NULL OR octet_length(request_hash) = 32);

COMMENT ON COLUMN security_space_epochs.request_hash IS
    'SHA-256 of the canonical member-revocation request; enables safe idempotent retries.';
