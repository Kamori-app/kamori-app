-- Synthetic byte canaries, not real user data or valid signed application messages.
INSERT INTO users (id, username, opaque_record, encrypted_master_key, public_key_bundle, recovery_verifier_hash)
VALUES ('00000000-0000-0000-0000-000000000001', 'restore-canary', decode('0011','hex'), decode(repeat('aa',49),'hex'), decode('bb','hex'), decode(repeat('cc',32),'hex'));
INSERT INTO workspaces (id, owner_user_id, kind, encrypted_metadata)
VALUES ('00000000-0000-0000-0000-000000000002','00000000-0000-0000-0000-000000000001','personal',decode('0044ff','hex'));
INSERT INTO devices (id,user_id,signing_public_key,hpke_public_key,encrypted_name,platform)
VALUES ('00000000-0000-0000-0000-000000000003','00000000-0000-0000-0000-000000000001',decode(repeat('11',32),'hex'),decode(repeat('22',32),'hex'),decode('00ff','hex'),'web');
INSERT INTO security_spaces (id,workspace_id,owner_user_id,encrypted_metadata,next_sequence)
VALUES ('00000000-0000-0000-0000-000000000004','00000000-0000-0000-0000-000000000002','00000000-0000-0000-0000-000000000001',decode('0055ff','hex'),1);
INSERT INTO operation_log (space_id,space_seq,stream_id,client_op_id,author_device_id,key_epoch,envelope_kind,cipher_suite,nonce,ciphertext,signature)
VALUES ('00000000-0000-0000-0000-000000000004',1,'00000000-0000-0000-0000-000000000005','00000000-0000-0000-0000-000000000006','00000000-0000-0000-0000-000000000003',1,'operation','xchacha20_poly1305',decode(repeat('33',24),'hex'),decode('000102ff0080aabbccddeeff','hex'),decode(repeat('44',64),'hex'));
CREATE TABLE readiness_markers (name text PRIMARY KEY, committed_at timestamptz NOT NULL DEFAULT clock_timestamp());
