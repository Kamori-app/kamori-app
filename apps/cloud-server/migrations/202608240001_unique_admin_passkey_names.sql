CREATE UNIQUE INDEX uq_admin_security_keys_user_name_ci
    ON admin_security_keys (admin_user_id, lower(name));
