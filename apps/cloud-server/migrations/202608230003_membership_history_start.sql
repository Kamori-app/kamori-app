ALTER TABLE security_space_members
    ADD COLUMN history_start_seq BIGINT NOT NULL DEFAULT 0
        CHECK (history_start_seq >= 0);

COMMENT ON COLUMN security_space_members.history_start_seq IS
    'Transport cursor immediately before the first operation this membership may decrypt.';

ALTER TABLE security_space_invites
    ADD COLUMN rotation_id UUID REFERENCES security_space_epochs(rotation_id);

CREATE UNIQUE INDEX idx_security_space_invites_rotation
    ON security_space_invites (rotation_id)
    WHERE rotation_id IS NOT NULL;

COMMENT ON COLUMN security_space_invites.rotation_id IS
    'Owner-prepared current-state key rotation consumed by this one-time invite.';
