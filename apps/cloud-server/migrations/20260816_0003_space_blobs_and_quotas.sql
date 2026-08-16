CREATE TABLE IF NOT EXISTS space_blobs (
    id UUID PRIMARY KEY,
    space_id UUID NOT NULL REFERENCES security_spaces(id) ON DELETE CASCADE,
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    ciphertext_sha256 BYTEA NOT NULL CHECK (octet_length(ciphertext_sha256) = 32),
    size_padded BIGINT NOT NULL CHECK (size_padded > 0),
    object_key TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'ready')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (space_id, id)
);

CREATE INDEX IF NOT EXISTS idx_space_blobs_owner_created
    ON space_blobs (owner_user_id, created_at);
CREATE INDEX IF NOT EXISTS idx_space_blobs_space_created
    ON space_blobs (space_id, created_at);

CREATE TABLE IF NOT EXISTS blob_egress_reservations (
    id UUID PRIMARY KEY,
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    requested_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    space_id UUID NOT NULL REFERENCES security_spaces(id) ON DELETE CASCADE,
    blob_id UUID NOT NULL REFERENCES space_blobs(id) ON DELETE CASCADE,
    bytes_reserved BIGINT NOT NULL CHECK (bytes_reserved > 0),
    reserved_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_blob_egress_owner_reserved
    ON blob_egress_reservations (owner_user_id, reserved_at);
CREATE INDEX IF NOT EXISTS idx_blob_egress_global_reserved
    ON blob_egress_reservations (reserved_at);

CREATE TABLE IF NOT EXISTS object_deletion_queue (
    id UUID PRIMARY KEY,
    object_key TEXT NOT NULL UNIQUE,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_attempt_at TIMESTAMPTZ,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_object_deletion_queue_retry
    ON object_deletion_queue (last_attempt_at, requested_at);
