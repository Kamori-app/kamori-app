//! Auth HTTP handlers.

mod device_authorization;
#[cfg(test)]
mod integration_tests;
mod passkeys;
mod password;
mod reauth;
mod sessions;
mod signin;
mod signup;
mod totp;

pub use device_authorization::{
    approve as device_authorization_approve, inspect as device_authorization_inspect,
    start as device_authorization_start, token as device_authorization_token,
};
pub use passkeys::{
    passkey_add_finish, passkey_add_start, passkey_delete, passkey_list, passkey_login_finish,
    passkey_login_start, passkey_update,
};
pub use password::{
    account_recovery_finish, account_recovery_start, password_change_finish, password_change_start,
};
pub use reauth::{finish as reauth_finish, start as reauth_start};
pub use sessions::{csrf_bootstrap, list_sessions, logout, refresh, revoke};
pub use signin::{signin_finish, signin_start, signin_totp};
pub use signup::{signup_finish, signup_start};
pub use totp::{
    account_recovery_codes_regenerate, totp_disable, totp_setup_finish, totp_setup_start,
    totp_status,
};
