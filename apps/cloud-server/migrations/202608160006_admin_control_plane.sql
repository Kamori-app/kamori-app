ALTER TABLE users
    ADD COLUMN suspended_at TIMESTAMPTZ,
    ADD COLUMN suspension_reason TEXT;

CREATE TABLE admin_users (
    id UUID PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    totp_secret_ciphertext BYTEA NOT NULL CHECK (octet_length(totp_secret_ciphertext) >= 42),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'active', 'suspended')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    activated_at TIMESTAMPTZ,
    last_login_at TIMESTAMPTZ
);

CREATE TABLE admin_security_keys (
    id UUID PRIMARY KEY,
    admin_user_id UUID NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (char_length(name) BETWEEN 3 AND 64),
    credential_id BYTEA NOT NULL UNIQUE,
    security_key_data BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ
);

CREATE INDEX idx_admin_security_keys_user
    ON admin_security_keys (admin_user_id);

CREATE TABLE admin_bootstrap_tokens (
    id UUID PRIMARY KEY,
    admin_user_id UUID NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE admin_sessions (
    id UUID PRIMARY KEY,
    admin_user_id UUID NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    session_kind TEXT NOT NULL CHECK (session_kind IN ('session', 'reauth')),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    user_agent TEXT,
    ip_address TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ
);

CREATE INDEX idx_admin_sessions_user_active
    ON admin_sessions (admin_user_id, expires_at)
    WHERE revoked_at IS NULL;

CREATE TABLE admin_audit_log (
    id UUID PRIMARY KEY,
    actor_admin_user_id UUID REFERENCES admin_users(id) ON DELETE SET NULL,
    event_kind TEXT NOT NULL,
    target_kind TEXT,
    target_id TEXT,
    reason TEXT,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    ip_address TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_admin_audit_created
    ON admin_audit_log (created_at DESC);

CREATE TABLE runtime_config_overrides (
    key TEXT PRIMARY KEY,
    value JSONB NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    updated_by UUID NOT NULL REFERENCES admin_users(id) ON DELETE RESTRICT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE operator_job_heartbeats (
    job_name TEXT PRIMARY KEY,
    status TEXT NOT NULL CHECK (status IN ('ok', 'warning', 'failed', 'running')),
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_started_at TIMESTAMPTZ,
    last_succeeded_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF EXISTS (SELECT FROM pg_roles WHERE rolname = 'kamori_jobs') THEN
        EXECUTE format('GRANT CONNECT ON DATABASE %I TO kamori_jobs', current_database());
        GRANT USAGE ON SCHEMA public TO kamori_jobs;
        GRANT SELECT, INSERT, UPDATE ON operator_job_heartbeats TO kamori_jobs;
    END IF;
END
$$;
