CREATE TABLE IF NOT EXISTS user_consents (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    product_analytics BOOLEAN NOT NULL DEFAULT FALSE,
    crash_reports BOOLEAN NOT NULL DEFAULT FALSE,
    marketing BOOLEAN NOT NULL DEFAULT FALSE,
    policy_version INTEGER NOT NULL CHECK (policy_version > 0),
    product_analytics_updated_at TIMESTAMPTZ,
    crash_reports_updated_at TIMESTAMPTZ,
    marketing_updated_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS user_consent_audit (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    product_analytics BOOLEAN NOT NULL,
    crash_reports BOOLEAN NOT NULL,
    marketing BOOLEAN NOT NULL,
    policy_version INTEGER NOT NULL CHECK (policy_version > 0),
    changed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_user_consent_audit_user_changed
    ON user_consent_audit (user_id, changed_at DESC);
