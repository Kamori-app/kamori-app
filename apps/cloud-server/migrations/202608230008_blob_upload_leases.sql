ALTER TABLE space_blobs
    ADD COLUMN upload_lease_until TIMESTAMPTZ;

UPDATE space_blobs
SET upload_lease_until = created_at + interval '15 minutes'
WHERE status = 'pending';

CREATE INDEX idx_space_blobs_pending_cleanup
    ON space_blobs (upload_lease_until, created_at)
    WHERE status = 'pending';
