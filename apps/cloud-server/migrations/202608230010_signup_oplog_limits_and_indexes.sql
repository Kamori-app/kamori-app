CREATE TABLE signup_completions (
    request_id UUID PRIMARY KEY,
    username TEXT NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE users
    ADD COLUMN operation_bytes BIGINT NOT NULL DEFAULT 0 CHECK (operation_bytes >= 0);
ALTER TABLE users
    ADD COLUMN blob_storage_bytes BIGINT NOT NULL DEFAULT 0 CHECK (blob_storage_bytes >= 0);
ALTER TABLE security_spaces
    ADD COLUMN operation_bytes BIGINT NOT NULL DEFAULT 0 CHECK (operation_bytes >= 0);

UPDATE security_spaces s
SET operation_bytes = usage.total
FROM (
    SELECT space_id,
           sum(octet_length(nonce) + octet_length(ciphertext) + octet_length(signature) + 192)::bigint AS total
    FROM operation_log
    GROUP BY space_id
) usage
WHERE usage.space_id = s.id;

UPDATE users u
SET operation_bytes = usage.total
FROM (
    SELECT s.owner_user_id,
           sum(s.operation_bytes)::bigint AS total
    FROM security_spaces s
    GROUP BY s.owner_user_id
) usage
WHERE usage.owner_user_id = u.id;

UPDATE users u
SET blob_storage_bytes = usage.total
FROM (
    SELECT owner_user_id, sum(size_padded)::bigint AS total
    FROM space_blobs
    GROUP BY owner_user_id
) usage
WHERE usage.owner_user_id = u.id;

CREATE FUNCTION adjust_operation_storage_usage() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    charged_bytes BIGINT;
    owner_id UUID;
BEGIN
    IF TG_OP = 'INSERT' THEN
        charged_bytes := octet_length(NEW.nonce) + octet_length(NEW.ciphertext) + octet_length(NEW.signature) + 192;
        SELECT owner_user_id INTO owner_id FROM security_spaces WHERE id = NEW.space_id;
        UPDATE security_spaces SET operation_bytes = operation_bytes + charged_bytes WHERE id = NEW.space_id;
        UPDATE users SET operation_bytes = operation_bytes + charged_bytes WHERE id = owner_id;
        RETURN NEW;
    END IF;
    charged_bytes := octet_length(OLD.nonce) + octet_length(OLD.ciphertext) + octet_length(OLD.signature) + 192;
    SELECT owner_user_id INTO owner_id FROM security_spaces WHERE id = OLD.space_id;
    UPDATE security_spaces SET operation_bytes = GREATEST(0, operation_bytes - charged_bytes) WHERE id = OLD.space_id;
    UPDATE users SET operation_bytes = GREATEST(0, operation_bytes - charged_bytes) WHERE id = owner_id;
    RETURN OLD;
END;
$$;

CREATE TRIGGER operation_storage_usage_insert
AFTER INSERT ON operation_log
FOR EACH ROW EXECUTE FUNCTION adjust_operation_storage_usage();
CREATE TRIGGER operation_storage_usage_delete
AFTER DELETE ON operation_log
FOR EACH ROW EXECUTE FUNCTION adjust_operation_storage_usage();

CREATE FUNCTION adjust_blob_storage_usage() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE users
        SET blob_storage_bytes = blob_storage_bytes + NEW.size_padded
        WHERE id = NEW.owner_user_id;
        RETURN NEW;
    END IF;
    UPDATE users
    SET blob_storage_bytes = GREATEST(0, blob_storage_bytes - OLD.size_padded)
    WHERE id = OLD.owner_user_id;
    RETURN OLD;
END;
$$;

-- Ownership transfer may update thousands of blob rows at once. Aggregate the
-- counter movement once per owner and statement instead of repeatedly updating
-- the same two user rows from a row-level trigger.
CREATE FUNCTION adjust_blob_storage_owner_update() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE users account
    SET blob_storage_bytes = GREATEST(0, account.blob_storage_bytes + movement.delta)
    FROM (
        SELECT owner_user_id, sum(delta)::bigint AS delta
        FROM (
            SELECT owner_user_id, -sum(size_padded)::bigint AS delta
            FROM old_blobs
            GROUP BY owner_user_id
            UNION ALL
            SELECT owner_user_id, sum(size_padded)::bigint AS delta
            FROM new_blobs
            GROUP BY owner_user_id
        ) changes
        GROUP BY owner_user_id
        HAVING sum(delta) <> 0
    ) movement
    WHERE account.id = movement.owner_user_id;
    RETURN NULL;
END;
$$;

CREATE TRIGGER blob_storage_usage_insert
AFTER INSERT ON space_blobs
FOR EACH ROW EXECUTE FUNCTION adjust_blob_storage_usage();
CREATE TRIGGER blob_storage_usage_delete
AFTER DELETE ON space_blobs
FOR EACH ROW EXECUTE FUNCTION adjust_blob_storage_usage();
CREATE TRIGGER blob_storage_usage_owner_update
AFTER UPDATE ON space_blobs
REFERENCING OLD TABLE AS old_blobs NEW TABLE AS new_blobs
FOR EACH STATEMENT EXECUTE FUNCTION adjust_blob_storage_owner_update();

-- Keep account ownership counters and denormalized blob ownership correct even
-- when ownership is changed by a future service or an operator transaction.
-- Admission limits are still checked by the application before this trigger;
-- the database owns the accounting invariant after the update is accepted.
CREATE FUNCTION transfer_security_space_usage() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.owner_user_id IS NOT DISTINCT FROM NEW.owner_user_id THEN
        RETURN NEW;
    END IF;
    UPDATE users
    SET operation_bytes = GREATEST(0, operation_bytes - NEW.operation_bytes)
    WHERE id = OLD.owner_user_id;
    UPDATE users
    SET operation_bytes = operation_bytes + NEW.operation_bytes
    WHERE id = NEW.owner_user_id;
    UPDATE space_blobs
    SET owner_user_id = NEW.owner_user_id
    WHERE space_id = NEW.id
      AND owner_user_id IS DISTINCT FROM NEW.owner_user_id;
    RETURN NEW;
END;
$$;

CREATE TRIGGER security_space_owner_usage_transfer
AFTER UPDATE OF owner_user_id ON security_spaces
FOR EACH ROW EXECUTE FUNCTION transfer_security_space_usage();

CREATE INDEX idx_operation_log_space_device_seq
    ON operation_log (space_id, author_device_id, space_seq);
