ALTER TABLE security_spaces
    ADD COLUMN current_state_start_seq BIGINT;

UPDATE security_spaces space
SET current_state_start_seq = COALESCE(
    (
        SELECT MIN(operation.space_seq) - 1
        FROM operation_log operation
        WHERE operation.space_id = space.id
          AND operation.key_epoch = space.current_key_epoch
    ),
    space.next_sequence
);

ALTER TABLE security_spaces
    ALTER COLUMN current_state_start_seq SET DEFAULT 0,
    ALTER COLUMN current_state_start_seq SET NOT NULL,
    ADD CONSTRAINT security_spaces_current_state_cursor_valid CHECK (
        current_state_start_seq >= 0
        AND current_state_start_seq <= next_sequence
    );

COMMENT ON COLUMN security_spaces.current_state_start_seq IS
    'Transport cursor immediately before the current key epoch; persisted at rotation so space listings never scan the operation log.';
