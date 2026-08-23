//! Auth repositories layer.
//!
//! SQL persistence helpers for auth flows.

mod account_recovery_attempts;
mod account_recovery_codes;
mod refresh_tokens;
mod user_passkeys;
mod users;

pub(crate) use account_recovery_attempts::{
    AccountRecoveryReset, apply_account_recovery_reset, create_account_recovery_attempt,
    find_account_recovery_attempt_user,
};
pub(crate) use account_recovery_codes::{
    consume_totp_backup_code, count_unused_recovery_codes,
    enable_totp_for_user_with_recovery_codes, regenerate_recovery_codes_for_user,
};
pub(crate) use refresh_tokens::{
    DeviceAuthorizationRefresh, RefreshRotation, create_device_authorization_refresh_token,
    create_refresh_token, list_refresh_sessions, revoke_refresh_token_by_hash,
    revoke_refresh_token_by_id_for_user, rotate_refresh_token,
    update_user_password_file_and_revoke_refresh_sessions,
};
pub(crate) use user_passkeys::{
    delete_passkey_for_user, get_user_and_passkey_by_credential_id, get_user_passkey,
    list_user_passkey_metadata, update_passkey_name_for_user, update_user_passkey,
    upsert_owned_user_passkey,
};
pub(crate) use users::{
    NewUser, UserAdmissionResult, UserRow, clear_totp_for_user, find_active_username_by_id,
    find_signup_completion, find_user_by_username, find_user_for_data_recovery,
    get_user_by_username, get_user_totp_ciphertext_by_id,
    insert_user_with_personal_workspace_and_admission_cap,
};
