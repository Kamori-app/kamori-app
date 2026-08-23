CREATE TABLE blob_egress_usage_buckets (
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('global', 'owner')),
    scope_id UUID NOT NULL,
    window_kind TEXT NOT NULL CHECK (window_kind IN ('month', 'quarter_hour')),
    window_start TIMESTAMPTZ NOT NULL,
    bytes_pending BIGINT NOT NULL DEFAULT 0 CHECK (bytes_pending >= 0),
    bytes_delivered BIGINT NOT NULL DEFAULT 0 CHECK (bytes_delivered >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (scope_kind, scope_id, window_kind, window_start),
    CHECK (
        (scope_kind = 'global' AND scope_id = '00000000-0000-0000-0000-000000000000')
        OR (scope_kind = 'owner' AND scope_id <> '00000000-0000-0000-0000-000000000000')
    )
);

CREATE INDEX idx_blob_egress_usage_owner_rolling
    ON blob_egress_usage_buckets (scope_id, window_start)
    WHERE scope_kind = 'owner' AND window_kind = 'quarter_hour';

-- Reservations abandoned by a dead gateway must not become permanent quota.
UPDATE blob_egress_reservations
SET completed_at = now(), bytes_delivered = 0
WHERE completed_at IS NULL AND reserved_at < now() - interval '1 hour';

INSERT INTO blob_egress_usage_buckets (
    scope_kind, scope_id, window_kind, window_start, bytes_pending, bytes_delivered
)
SELECT
    'owner', owner_user_id, 'month',
    date_trunc('month', reserved_at AT TIME ZONE 'UTC') AT TIME ZONE 'UTC',
    COALESCE(sum(bytes_reserved) FILTER (WHERE completed_at IS NULL), 0)::bigint,
    COALESCE(sum(bytes_delivered) FILTER (WHERE completed_at IS NOT NULL), 0)::bigint
FROM blob_egress_reservations
GROUP BY owner_user_id,
    date_trunc('month', reserved_at AT TIME ZONE 'UTC') AT TIME ZONE 'UTC';

INSERT INTO blob_egress_usage_buckets (
    scope_kind, scope_id, window_kind, window_start, bytes_pending, bytes_delivered
)
SELECT
    'owner', owner_user_id, 'quarter_hour',
    date_bin(interval '15 minutes', reserved_at, timestamptz '2000-01-01 00:00:00+00'),
    COALESCE(sum(bytes_reserved) FILTER (WHERE completed_at IS NULL), 0)::bigint,
    COALESCE(sum(bytes_delivered) FILTER (WHERE completed_at IS NOT NULL), 0)::bigint
FROM blob_egress_reservations
GROUP BY owner_user_id,
    date_bin(interval '15 minutes', reserved_at, timestamptz '2000-01-01 00:00:00+00');

INSERT INTO blob_egress_usage_buckets (
    scope_kind, scope_id, window_kind, window_start, bytes_pending, bytes_delivered
)
SELECT
    'global', '00000000-0000-0000-0000-000000000000', 'month',
    date_trunc('month', reserved_at AT TIME ZONE 'UTC') AT TIME ZONE 'UTC',
    COALESCE(sum(bytes_reserved) FILTER (WHERE completed_at IS NULL), 0)::bigint,
    COALESCE(sum(bytes_delivered) FILTER (WHERE completed_at IS NOT NULL), 0)::bigint
FROM blob_egress_reservations
GROUP BY date_trunc('month', reserved_at AT TIME ZONE 'UTC') AT TIME ZONE 'UTC';

CREATE INDEX idx_blob_egress_incomplete_expiry
    ON blob_egress_reservations (reserved_at)
    WHERE completed_at IS NULL;
