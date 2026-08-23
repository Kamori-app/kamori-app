ALTER TABLE security_space_invites
    ADD COLUMN request_hash BYTEA;

ALTER TABLE security_space_invites
    ADD CONSTRAINT security_space_invites_request_hash_size
        CHECK (request_hash IS NULL OR octet_length(request_hash) = 32);

COMMENT ON COLUMN security_space_invites.request_hash IS
    'SHA-256 of the canonical invite request used for exact idempotent retries.';
