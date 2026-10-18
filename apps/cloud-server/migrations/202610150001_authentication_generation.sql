-- Deferred authentication is valid only for the credential state that authorized it.
ALTER TABLE users ADD COLUMN auth_generation BIGINT NOT NULL DEFAULT 0;
