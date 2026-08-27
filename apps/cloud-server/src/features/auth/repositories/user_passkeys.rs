//! Repository functions for the `user_passkeys` table.

use sqlx::{PgPool, Row};
use uuid::Uuid;
use webauthn_rs::prelude::Passkey;

use crate::{features::auth::dto::PasskeyMetadata, platform::security::passkey::decode_passkey};

use crate::features::common::{ApiError, bad_request, internal_error, unauthenticated};

use super::users::{UserRow, map_user_row};

/// Fetches a user and passkey by credential id.
pub(crate) async fn get_user_and_passkey_by_credential_id(
    pool: &PgPool,
    credential_id: &[u8],
) -> Result<(UserRow, Passkey), ApiError> {
    let sql = r#"SELECT u.id, u.username, u.opaque_record, u.totp_secret_ciphertext, u.encrypted_master_key, u.public_key_bundle, up.passkey_data
               FROM user_passkeys up
               JOIN users u ON u.id = up.user_id
               WHERE up.credential_id = $1 AND u.deleted_at IS NULL AND u.suspended_at IS NULL"#;

    let row = sqlx::query(sql)
        .bind(credential_id)
        .fetch_optional(pool)
        .await
        .map_err(internal_error)?;

    let row = row.ok_or_else(|| unauthenticated("passkey credential not found"))?;
    let user = map_user_row(&row)?;
    let passkey_data: Vec<u8> = row.try_get("passkey_data").map_err(internal_error)?;
    let passkey = decode_passkey(&passkey_data).map_err(internal_error)?;
    Ok((user, passkey))
}

fn map_passkey_metadata(row: &sqlx::postgres::PgRow) -> Result<PasskeyMetadata, ApiError> {
    let id: Uuid = row.try_get("id").map_err(internal_error)?;
    let credential_id: Vec<u8> = row.try_get("credential_id").map_err(internal_error)?;
    let encrypted_name: Vec<u8> = row.try_get("encrypted_name").map_err(internal_error)?;
    Ok(PasskeyMetadata {
        id,
        credential_id,
        encrypted_name,
    })
}

/// Inserts or updates a user passkey, but never allows cross-user credential takeover.
pub(crate) async fn upsert_owned_user_passkey(
    pool: &PgPool,
    user_id: Uuid,
    credential_id: &[u8],
    passkey_data: &[u8],
    encrypted_name: &[u8],
) -> Result<PasskeyMetadata, ApiError> {
    let row = sqlx::query(
        r#"
        INSERT INTO user_passkeys (id, user_id, credential_id, passkey_data, encrypted_name)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (credential_id)
        DO UPDATE SET
            passkey_data = EXCLUDED.passkey_data,
            encrypted_name = EXCLUDED.encrypted_name,
            updated_at = now()
        WHERE user_passkeys.user_id = EXCLUDED.user_id
        RETURNING id, credential_id, encrypted_name
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(credential_id)
    .bind(passkey_data)
    .bind(encrypted_name)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?;

    let row = row.ok_or_else(|| bad_request("credential is already linked to another account"))?;
    map_passkey_metadata(&row)
}

/// Lists stored passkeys for the user.
pub(crate) async fn list_user_passkey_metadata(
    pool: &PgPool,
    user_id: Uuid,
) -> anyhow::Result<Vec<PasskeyMetadata>> {
    let rows = sqlx::query(
        r#"
        SELECT id, credential_id, encrypted_name
        FROM user_passkeys
        WHERE user_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: Uuid = row.try_get("id")?;
        let credential_id: Vec<u8> = row.try_get("credential_id")?;
        let encrypted_name: Vec<u8> = row.try_get("encrypted_name")?;
        out.push(PasskeyMetadata {
            id,
            credential_id,
            encrypted_name,
        });
    }
    Ok(out)
}

/// Updates encrypted passkey name and returns updated metadata.
pub(crate) async fn update_passkey_name_for_user(
    pool: &PgPool,
    user_id: Uuid,
    passkey_id: Uuid,
    encrypted_name: &[u8],
) -> anyhow::Result<Option<PasskeyMetadata>> {
    let row = sqlx::query(
        r#"
        UPDATE user_passkeys
        SET encrypted_name = $3, updated_at = now()
        WHERE id = $1 AND user_id = $2
        RETURNING id, credential_id, encrypted_name
        "#,
    )
    .bind(passkey_id)
    .bind(user_id)
    .bind(encrypted_name)
    .fetch_optional(pool)
    .await?;

    let metadata = if let Some(row) = row {
        let id: Uuid = row.try_get("id")?;
        let credential_id: Vec<u8> = row.try_get("credential_id")?;
        let encrypted_name: Vec<u8> = row.try_get("encrypted_name")?;
        Some(PasskeyMetadata {
            id,
            credential_id,
            encrypted_name,
        })
    } else {
        None
    };
    Ok(metadata)
}

/// Deletes passkey by id for the specific user, including their final passkey.
/// OPAQUE password authentication is independent and remains available.
pub(crate) async fn delete_passkey_for_user(
    pool: &PgPool,
    user_id: Uuid,
    passkey_id: Uuid,
) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM user_passkeys WHERE id = $1 AND user_id = $2")
        .bind(passkey_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Loads a specific passkey by user and credential id.
pub(crate) async fn get_user_passkey(
    pool: &PgPool,
    user_id: Uuid,
    credential_id: &[u8],
) -> Result<Passkey, ApiError> {
    let row = sqlx::query(
        "SELECT passkey_data FROM user_passkeys WHERE user_id = $1 AND credential_id = $2",
    )
    .bind(user_id)
    .bind(credential_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| unauthenticated("passkey credential not found"))?;

    let bytes: Vec<u8> = row.try_get("passkey_data").map_err(internal_error)?;
    decode_passkey(&bytes).map_err(internal_error)
}

/// Persists passkey updates after authentication counter/backup changes.
pub(crate) async fn update_user_passkey(
    pool: &PgPool,
    user_id: Uuid,
    credential_id: &[u8],
    passkey_data: &[u8],
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE user_passkeys SET passkey_data = $3, updated_at = now() WHERE user_id = $1 AND credential_id = $2",
    )
    .bind(user_id)
    .bind(credential_id)
    .bind(passkey_data)
    .execute(pool)
    .await?;

    Ok(())
}
