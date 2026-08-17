CREATE TABLE server_security_config (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    opaque_setup_version INTEGER NOT NULL CHECK (opaque_setup_version > 0),
    opaque_setup_fingerprint BYTEA NOT NULL CHECK (octet_length(opaque_setup_fingerprint) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE refresh_tokens
    ADD COLUMN rotation_request_id UUID,
    ADD COLUMN rotated_at TIMESTAMPTZ;

CREATE INDEX idx_refresh_tokens_rotation_request
    ON refresh_tokens (id, rotation_request_id)
    WHERE rotation_request_id IS NOT NULL;

ALTER TABLE users
    ADD COLUMN opaque_setup_version INTEGER NOT NULL DEFAULT 1
        CHECK (opaque_setup_version > 0);

ALTER TABLE security_space_invites
    ADD COLUMN key_epoch INTEGER,
    ADD COLUMN invite_version INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN max_uses INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN used_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN revoked_at TIMESTAMPTZ;

UPDATE security_space_invites invite
SET key_epoch = space.current_key_epoch
FROM security_spaces space
WHERE space.id = invite.space_id AND invite.key_epoch IS NULL;

ALTER TABLE security_space_invites
    ALTER COLUMN key_epoch SET NOT NULL,
    ADD CONSTRAINT security_space_invites_key_epoch_positive CHECK (key_epoch > 0),
    ADD CONSTRAINT security_space_invites_version_positive CHECK (invite_version > 0),
    ADD CONSTRAINT security_space_invites_usage_valid CHECK (
        max_uses > 0 AND used_count >= 0 AND used_count <= max_uses
    );

CREATE INDEX idx_security_space_invites_active_epoch
    ON security_space_invites (space_id, key_epoch, expires_at)
    WHERE revoked_at IS NULL AND used_count < max_uses;

CREATE TABLE security_space_epochs (
    space_id UUID NOT NULL REFERENCES security_spaces(id) ON DELETE CASCADE,
    key_epoch INTEGER NOT NULL CHECK (key_epoch > 0),
    rotation_id UUID NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK (status IN ('preparing', 'committed', 'superseded')),
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    committed_at TIMESTAMPTZ,
    superseded_at TIMESTAMPTZ,
    PRIMARY KEY (space_id, key_epoch)
);

INSERT INTO security_space_epochs (
    space_id, key_epoch, rotation_id, status, created_by, committed_at
)
SELECT id, current_key_epoch, gen_random_uuid(), 'committed', created_by, created_at
FROM security_spaces;

ALTER TABLE blob_egress_reservations
    ADD COLUMN bytes_delivered BIGINT NOT NULL DEFAULT 0
        CHECK (bytes_delivered >= 0 AND bytes_delivered <= bytes_reserved),
    ADD COLUMN completed_at TIMESTAMPTZ;

CREATE INDEX idx_blob_egress_active_requester
    ON blob_egress_reservations (requested_by, reserved_at)
    WHERE completed_at IS NULL;
