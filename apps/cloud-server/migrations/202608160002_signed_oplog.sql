CREATE TABLE devices (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    signing_public_key BYTEA NOT NULL CHECK (octet_length(signing_public_key) = 32),
    hpke_public_key BYTEA NOT NULL CHECK (octet_length(hpke_public_key) = 32),
    encrypted_name BYTEA NOT NULL,
    platform TEXT NOT NULL CHECK (platform IN ('web', 'desktop', 'android', 'ios')),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);

CREATE INDEX idx_devices_user_active
    ON devices (user_id)
    WHERE status = 'active';

ALTER TABLE devices ADD CONSTRAINT devices_id_user_unique UNIQUE (id, user_id);

CREATE TABLE security_spaces (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    encrypted_metadata BYTEA NOT NULL,
    current_key_epoch INTEGER NOT NULL DEFAULT 1 CHECK (current_key_epoch > 0),
    next_sequence BIGINT NOT NULL DEFAULT 0 CHECK (next_sequence >= 0),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'deleted')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_security_spaces_workspace_active
    ON security_spaces (workspace_id)
    WHERE status = 'active';

CREATE INDEX idx_security_spaces_owner_active
    ON security_spaces (owner_user_id)
    WHERE status = 'active';

CREATE TABLE security_space_members (
    id UUID PRIMARY KEY,
    space_id UUID NOT NULL REFERENCES security_spaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('owner', 'editor', 'reader')),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
    key_epoch INTEGER NOT NULL CHECK (key_epoch > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ,
    UNIQUE (space_id, user_id)
);

CREATE INDEX idx_security_space_members_user_active
    ON security_space_members (user_id, space_id)
    WHERE status = 'active';

CREATE TABLE security_space_device_keys (
    space_id UUID NOT NULL REFERENCES security_spaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id UUID NOT NULL,
    key_epoch INTEGER NOT NULL CHECK (key_epoch > 0),
    encrypted_key_package BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (space_id, device_id, key_epoch),
    FOREIGN KEY (device_id, user_id) REFERENCES devices(id, user_id) ON DELETE CASCADE,
    FOREIGN KEY (space_id, user_id) REFERENCES security_space_members(space_id, user_id) ON DELETE CASCADE
);

CREATE INDEX idx_security_space_device_keys_user
    ON security_space_device_keys (user_id, device_id, space_id);

CREATE TABLE operation_log (
    space_id UUID NOT NULL REFERENCES security_spaces(id) ON DELETE CASCADE,
    space_seq BIGINT NOT NULL CHECK (space_seq > 0),
    stream_id UUID NOT NULL,
    client_op_id UUID NOT NULL,
    author_device_id UUID NOT NULL REFERENCES devices(id) ON DELETE RESTRICT,
    key_epoch INTEGER NOT NULL CHECK (key_epoch > 0),
    envelope_kind TEXT NOT NULL CHECK (envelope_kind IN ('operation', 'snapshot', 'control')),
    cipher_suite TEXT NOT NULL CHECK (cipher_suite IN ('xchacha20_poly1305', 'aes256_gcm')),
    nonce BYTEA NOT NULL,
    ciphertext BYTEA NOT NULL,
    signature BYTEA NOT NULL CHECK (octet_length(signature) = 64),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (space_id, space_seq),
    UNIQUE (space_id, client_op_id)
);

CREATE INDEX idx_operation_log_space_stream_seq
    ON operation_log (space_id, stream_id, space_seq);

CREATE INDEX idx_operation_log_device
    ON operation_log (author_device_id);

CREATE TABLE security_space_invites (
    id UUID PRIMARY KEY,
    space_id UUID NOT NULL REFERENCES security_spaces(id) ON DELETE CASCADE,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('editor', 'reader')),
    code_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(code_hash) = 32),
    encrypted_key_package BYTEA NOT NULL,
    encrypted_note BYTEA,
    expires_at TIMESTAMPTZ NOT NULL,
    redeemed_by UUID REFERENCES users(id) ON DELETE SET NULL,
    redeemed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_security_space_invites_expiry
    ON security_space_invites (expires_at)
    WHERE redeemed_at IS NULL;

CREATE TABLE security_events (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_kind TEXT NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_security_events_user_created
    ON security_events (user_id, created_at DESC);
