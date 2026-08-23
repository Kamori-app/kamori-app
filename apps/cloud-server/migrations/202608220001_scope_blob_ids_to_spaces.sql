-- Blob ids are client-generated opaque identifiers. They are only required to
-- be unique inside a security space; a global primary key unnecessarily joins
-- otherwise independent tenant namespaces and turns a known id into a
-- cross-space denial-of-service primitive.

ALTER TABLE blob_egress_reservations
    DROP CONSTRAINT IF EXISTS blob_egress_reservations_blob_id_fkey;

ALTER TABLE space_blobs
    DROP CONSTRAINT IF EXISTS space_blobs_pkey,
    DROP CONSTRAINT IF EXISTS space_blobs_space_id_id_key;

ALTER TABLE space_blobs
    ADD CONSTRAINT space_blobs_pkey PRIMARY KEY (space_id, id);

ALTER TABLE blob_egress_reservations
    ADD CONSTRAINT blob_egress_reservations_space_blob_fkey
    FOREIGN KEY (space_id, blob_id)
    REFERENCES space_blobs(space_id, id)
    ON DELETE CASCADE;
