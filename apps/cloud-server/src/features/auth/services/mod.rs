//! Auth services layer.

mod device_authorization;
mod passkeys;
mod password;
mod reauth;
mod sessions;
mod signin;
mod signup;
pub(crate) mod support;
mod totp;

pub(crate) use device_authorization::{
    approve as device_authorization_approve, start as device_authorization_start,
    token as device_authorization_token,
};
pub(crate) use passkeys::{
    passkey_add_finish, passkey_add_start, passkey_delete, passkey_list, passkey_login_finish,
    passkey_login_start, passkey_update,
};
pub(crate) use password::{
    account_recovery_finish, account_recovery_start, password_change_finish, password_change_start,
};
pub(crate) use reauth::{consume_reauth_token, finish as reauth_finish, start as reauth_start};
pub(crate) use sessions::{list_sessions, logout, refresh, revoke};
pub(crate) use signin::{signin_finish, signin_start};
pub(crate) use signup::{signup_finish, signup_start};
pub(crate) use totp::{
    account_recovery_codes_regenerate, totp_disable, totp_setup_finish, totp_setup_start,
    totp_status,
};
