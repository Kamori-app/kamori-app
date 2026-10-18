-- Exclude row-locking transactions before any trigger DDL or repair. Appends
-- lock space before account/device/operation work; acquire the strongest table
-- locks in that same order so admitted appends finish before repair starts.
-- EXCLUSIVE blocks both ROW SHARE (SELECT FOR UPDATE) and ROW EXCLUSIVE writes
-- while permitting ordinary readers. Taking this before CREATE TRIGGER avoids
-- a SHARE ROW EXCLUSIVE-to-EXCLUSIVE upgrade window on security_spaces.
LOCK TABLE security_spaces, operation_log, users IN EXCLUSIVE MODE;

-- The parent row is no longer visible to operation_log's AFTER DELETE
-- trigger during a cascade. Release its aggregate exactly once before removal;
-- direct operation deletes still update both the live parent and its owner.
CREATE FUNCTION release_security_space_operation_usage() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE users
    SET operation_bytes = GREATEST(0, operation_bytes - OLD.operation_bytes)
    WHERE id = OLD.owner_user_id;
    RETURN OLD;
END;
$$;

CREATE TRIGGER security_space_operation_usage_delete
BEFORE DELETE ON security_spaces
FOR EACH ROW EXECUTE FUNCTION release_security_space_operation_usage();

-- Repair pre-existing drift, including accounts with no remaining spaces.
-- Derive totals from live rows under the exclusion acquired above.
UPDATE security_spaces s
SET operation_bytes = COALESCE((
    SELECT sum(octet_length(nonce) + octet_length(ciphertext) + octet_length(signature) + 192)::bigint
    FROM operation_log WHERE space_id = s.id
), 0);
UPDATE users u
SET operation_bytes = COALESCE((
    SELECT sum(operation_bytes)::bigint
    FROM security_spaces WHERE owner_user_id = u.id
), 0);

-- Persist the effective first redemption response: existing active members can
-- have a stronger role or earlier history floor than the invite would grant.
-- Old consumed invites remain fail-closed because their exact response was
-- never recorded and cannot safely be reconstructed after entitlement changes.
ALTER TABLE security_space_invites
    ADD COLUMN redemption_role TEXT CHECK (redemption_role IN ('editor', 'reader')),
    ADD COLUMN redemption_history_start_seq BIGINT CHECK (redemption_history_start_seq >= 0),
    ADD COLUMN redemption_current_state_start_seq BIGINT CHECK (redemption_current_state_start_seq >= 0);
