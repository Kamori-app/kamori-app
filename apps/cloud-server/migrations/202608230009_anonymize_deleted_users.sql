-- Deleted accounts retain a stable pseudonymous row for shared encrypted
-- history and quota ledgers, but must not retain authentication or recovery
-- material. Active accounts continue to satisfy the original invariants.
ALTER TABLE users
    ALTER COLUMN opaque_record DROP NOT NULL,
    ALTER COLUMN encrypted_master_key DROP NOT NULL,
    ALTER COLUMN public_key_bundle DROP NOT NULL,
    ALTER COLUMN recovery_verifier_hash DROP NOT NULL;

ALTER TABLE users
    DROP CONSTRAINT IF EXISTS users_encrypted_master_key_check,
    DROP CONSTRAINT IF EXISTS users_public_key_bundle_check,
    DROP CONSTRAINT IF EXISTS users_recovery_verifier_hash_check;

ALTER TABLE users
    ADD CONSTRAINT users_active_opaque_record_check CHECK (
        deleted_at IS NOT NULL OR opaque_record IS NOT NULL
    ),
    ADD CONSTRAINT users_active_encrypted_master_key_check CHECK (
        deleted_at IS NOT NULL OR (
            encrypted_master_key IS NOT NULL
            AND octet_length(encrypted_master_key) BETWEEN 49 AND 65536
        )
    ),
    ADD CONSTRAINT users_active_public_key_bundle_check CHECK (
        deleted_at IS NOT NULL OR (
            public_key_bundle IS NOT NULL
            AND octet_length(public_key_bundle) BETWEEN 1 AND 65536
        )
    ),
    ADD CONSTRAINT users_active_recovery_verifier_hash_check CHECK (
        deleted_at IS NOT NULL OR (
            recovery_verifier_hash IS NOT NULL
            AND octet_length(recovery_verifier_hash) = 32
        )
    );
