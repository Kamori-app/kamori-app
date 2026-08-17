CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    opaque_record BYTEA NOT NULL,
    encrypted_master_key BYTEA NOT NULL CHECK (
        octet_length(encrypted_master_key) BETWEEN 49 AND 65536
    ),
    public_key_bundle BYTEA NOT NULL CHECK (
        octet_length(public_key_bundle) BETWEEN 1 AND 65536
    ),
    recovery_verifier_hash BYTEA NOT NULL CHECK (octet_length(recovery_verifier_hash) = 32),
    totp_secret_ciphertext BYTEA CHECK (
        totp_secret_ciphertext IS NULL OR octet_length(totp_secret_ciphertext) >= 42
    ),
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS workspaces (
    id UUID PRIMARY KEY,
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('personal', 'team')),
    encrypted_metadata BYTEA NOT NULL,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_workspaces_owner_user_id
    ON workspaces (owner_user_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_workspaces_personal_owner_unique
    ON workspaces (owner_user_id)
    WHERE kind = 'personal' AND deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS workspace_members (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member')),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(workspace_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_workspace_members_user_id
    ON workspace_members (user_id);

CREATE INDEX IF NOT EXISTS idx_workspace_members_workspace_active
    ON workspace_members (workspace_id)
    WHERE status = 'active';

CREATE TABLE IF NOT EXISTS user_passkeys (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    credential_id BYTEA NOT NULL UNIQUE,
    passkey_data BYTEA NOT NULL,
    encrypted_name BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_user_passkeys_user_id
    ON user_passkeys (user_id);

CREATE TABLE IF NOT EXISTS refresh_tokens (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    replaced_by_token_id UUID REFERENCES refresh_tokens(id) ON DELETE SET NULL,
    user_agent TEXT,
    ip_address TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id
    ON refresh_tokens (user_id);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires_at
    ON refresh_tokens (expires_at);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_active
    ON refresh_tokens (user_id)
    WHERE revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS account_recovery_codes (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash BYTEA NOT NULL CHECK (octet_length(code_hash) = 32),
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(user_id, code_hash)
);

CREATE INDEX IF NOT EXISTS idx_account_recovery_codes_user_id
    ON account_recovery_codes (user_id);

CREATE INDEX IF NOT EXISTS idx_account_recovery_codes_user_unused
    ON account_recovery_codes (user_id)
    WHERE used_at IS NULL;
