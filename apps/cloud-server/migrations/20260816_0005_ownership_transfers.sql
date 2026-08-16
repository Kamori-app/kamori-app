CREATE TABLE ownership_transfer_offers (
    id UUID PRIMARY KEY,
    resource_kind TEXT NOT NULL CHECK (resource_kind IN ('workspace', 'security_space')),
    resource_id UUID NOT NULL,
    current_owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    target_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (target_user_id <> current_owner_id),
    CHECK (accepted_at IS NULL OR cancelled_at IS NULL)
);

CREATE UNIQUE INDEX idx_ownership_transfer_one_pending_resource
    ON ownership_transfer_offers (resource_kind, resource_id)
    WHERE accepted_at IS NULL AND cancelled_at IS NULL;

CREATE INDEX idx_ownership_transfer_target_pending
    ON ownership_transfer_offers (target_user_id, created_at DESC)
    WHERE accepted_at IS NULL AND cancelled_at IS NULL;
