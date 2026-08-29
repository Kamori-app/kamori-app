//! Repository functions for the `refresh_tokens` table and related session reset flows.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use rand::RngExt;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::features::auth::dto::SessionSummary;
use crate::features::common::{ApiError, internal_error, unauthenticated};

#[derive(Debug)]
pub(crate) struct IssuedRefreshToken {
    pub(crate) token_id: Uuid,
    pub(crate) token: String,
}

#[derive(Debug)]
pub(crate) struct RotatedRefreshToken {
    pub(crate) user_id: Uuid,
    pub(crate) username: String,
    pub(crate) new_token_id: Uuid,
    pub(crate) new_token: String,
}

pub(crate) struct RefreshRotation<'a> {
    pub(crate) current_token_hash: &'a [u8],
    pub(crate) current_token: &'a str,
    pub(crate) rotation_request_id: Uuid,
    pub(crate) rotation_key: &'a [u8; 32],
    pub(crate) user_agent: Option<&'a str>,
    pub(crate) ip_address: Option<&'a str>,
    pub(crate) expires_at: OffsetDateTime,
}

pub(crate) struct DeviceAuthorizationRefresh<'a> {
    pub(crate) user_id: Uuid,
    pub(crate) device_secret: &'a str,
    pub(crate) flow_id: Uuid,
    pub(crate) rotation_key: &'a [u8; 32],
    pub(crate) user_agent: Option<&'a str>,
    pub(crate) ip_address: Option<&'a str>,
    pub(crate) expires_at: OffsetDateTime,
}

#[derive(Debug)]
struct StoredRefreshToken {
    id: Uuid,
    user_id: Uuid,
    expires_at: OffsetDateTime,
    revoked_at: Option<OffsetDateTime>,
    replaced_by_token_id: Option<Uuid>,
    rotation_request_id: Option<Uuid>,
    device_id: Option<Uuid>,
}

fn generate_refresh_token_and_hash() -> (String, Vec<u8>) {
    let mut raw = [0u8; 32];
    let mut rng = rand::rng();
    rng.fill(&mut raw);
    let token = URL_SAFE_NO_PAD.encode(raw);
    let token_hash = Sha256::digest(raw).to_vec();
    (token, token_hash)
}

fn derive_refresh_token(
    current_token: &str,
    rotation_request_id: Uuid,
    rotation_key: &[u8; 32],
) -> (String, Vec<u8>) {
    let mut mac = Hmac::<Sha256>::new_from_slice(rotation_key).expect("valid HMAC key");
    mac.update(b"kamori.refresh-rotation.v1\0");
    mac.update(current_token.as_bytes());
    mac.update(rotation_request_id.as_bytes());
    let raw = mac.finalize().into_bytes();
    let token = URL_SAFE_NO_PAD.encode(raw);
    let token_hash = Sha256::digest(raw).to_vec();
    (token, token_hash)
}

fn derive_device_authorization_refresh_token(
    device_secret: &str,
    flow_id: Uuid,
    rotation_key: &[u8; 32],
) -> (String, Vec<u8>) {
    let mut mac = Hmac::<Sha256>::new_from_slice(rotation_key).expect("valid HMAC key");
    mac.update(b"kamori.device-authorization-refresh.v1\0");
    mac.update(flow_id.as_bytes());
    mac.update(device_secret.as_bytes());
    let raw = mac.finalize().into_bytes();
    let token = URL_SAFE_NO_PAD.encode(raw);
    let token_hash = Sha256::digest(raw).to_vec();
    (token, token_hash)
}

pub(crate) async fn create_refresh_token(
    pool: &PgPool,
    user_id: Uuid,
    user_agent: Option<&str>,
    ip_address: Option<&str>,
    expires_at: OffsetDateTime,
) -> Result<IssuedRefreshToken, ApiError> {
    let (token, token_hash) = generate_refresh_token_and_hash();
    let token_id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO refresh_tokens (
            id, user_id, token_hash, expires_at, user_agent, ip_address
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(token_id)
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .bind(user_agent)
    .bind(ip_address)
    .execute(pool)
    .await
    .map_err(internal_error)?;

    Ok(IssuedRefreshToken { token_id, token })
}

/// Creates one stable refresh session for a device-authorization flow.
/// Retries after a lost HTTP response resolve the existing active row instead
/// of allocating orphaned sessions or storing plaintext tokens in Valkey.
pub(crate) async fn create_device_authorization_refresh_token(
    pool: &PgPool,
    request: DeviceAuthorizationRefresh<'_>,
) -> Result<IssuedRefreshToken, ApiError> {
    let DeviceAuthorizationRefresh {
        user_id,
        device_secret,
        flow_id,
        rotation_key,
        user_agent,
        ip_address,
        expires_at,
    } = request;
    let (token, token_hash) =
        derive_device_authorization_refresh_token(device_secret, flow_id, rotation_key);
    let proposed_token_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO refresh_tokens (
            id, user_id, token_hash, expires_at, user_agent, ip_address
        )
        SELECT $1, active_user.id, $3, $4, $5, $6
        FROM users active_user
        WHERE active_user.id = $2
          AND active_user.deleted_at IS NULL
          AND active_user.suspended_at IS NULL
        ON CONFLICT (token_hash) DO NOTHING
        "#,
    )
    .bind(proposed_token_id)
    .bind(user_id)
    .bind(&token_hash)
    .bind(expires_at)
    .bind(user_agent)
    .bind(ip_address)
    .execute(pool)
    .await
    .map_err(internal_error)?;

    let token_id: Uuid = sqlx::query_scalar(
        r#"
        SELECT token.id
        FROM refresh_tokens token
        JOIN users active_user ON active_user.id = token.user_id
        WHERE token.token_hash = $1
          AND token.user_id = $2
          AND token.revoked_at IS NULL
          AND token.expires_at > now()
          AND active_user.deleted_at IS NULL
          AND active_user.suspended_at IS NULL
        "#,
    )
    .bind(&token_hash)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| unauthenticated("device authorization was already consumed"))?;

    Ok(IssuedRefreshToken { token_id, token })
}

async fn find_refresh_token_for_update(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    token_hash: &[u8],
) -> Result<Option<StoredRefreshToken>, ApiError> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, expires_at, revoked_at, replaced_by_token_id,
               rotation_request_id, device_id
        FROM refresh_tokens
        WHERE token_hash = $1
        FOR UPDATE
        "#,
    )
    .bind(token_hash)
    .fetch_optional(&mut **tx)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Ok(None);
    };

    Ok(Some(StoredRefreshToken {
        id: row.try_get("id").map_err(internal_error)?,
        user_id: row.try_get("user_id").map_err(internal_error)?,
        expires_at: row.try_get("expires_at").map_err(internal_error)?,
        revoked_at: row.try_get("revoked_at").map_err(internal_error)?,
        replaced_by_token_id: row
            .try_get("replaced_by_token_id")
            .map_err(internal_error)?,
        rotation_request_id: row.try_get("rotation_request_id").map_err(internal_error)?,
        device_id: row.try_get("device_id").map_err(internal_error)?,
    }))
}

pub(crate) async fn rotate_refresh_token(
    pool: &PgPool,
    request: RefreshRotation<'_>,
) -> Result<RotatedRefreshToken, ApiError> {
    let RefreshRotation {
        current_token_hash,
        current_token,
        rotation_request_id,
        rotation_key,
        user_agent,
        ip_address,
        expires_at,
    } = request;
    let now = OffsetDateTime::now_utc();
    let mut tx = pool.begin().await.map_err(internal_error)?;

    let current = find_refresh_token_for_update(&mut tx, current_token_hash)
        .await?
        .ok_or_else(|| unauthenticated("invalid refresh token"))?;

    if current.revoked_at.is_some() {
        if current.rotation_request_id == Some(rotation_request_id)
            && let Some(replacement_id) = current.replaced_by_token_id
        {
            let username = sqlx::query_scalar::<_, String>(
                r#"
                SELECT u.username
                FROM refresh_tokens rt
                JOIN users u ON u.id = rt.user_id
                WHERE rt.id = $1
                  AND rt.revoked_at IS NULL
                  AND rt.expires_at > $2
                  AND u.deleted_at IS NULL
                  AND u.suspended_at IS NULL
                "#,
            )
            .bind(replacement_id)
            .bind(now)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal_error)?;
            if let Some(username) = username {
                let (new_token, _) =
                    derive_refresh_token(current_token, rotation_request_id, rotation_key);
                tx.commit().await.map_err(internal_error)?;
                return Ok(RotatedRefreshToken {
                    user_id: current.user_id,
                    username,
                    new_token_id: replacement_id,
                    new_token,
                });
            }
        }
        if current.replaced_by_token_id.is_some() {
            sqlx::query(
                "UPDATE refresh_tokens SET revoked_at = COALESCE(revoked_at, $2) WHERE user_id = $1",
            )
            .bind(current.user_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;
            sqlx::query(
                r#"
                INSERT INTO security_events (id, user_id, event_kind, details)
                VALUES ($1, $2, 'refresh_token_reuse', jsonb_build_object(
                    'refresh_token_id', $3::text,
                    'user_agent', $4::text,
                    'ip_address', $5::text
                ))
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(current.user_id)
            .bind(current.id)
            .bind(user_agent)
            .bind(ip_address)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;
            tx.commit().await.map_err(internal_error)?;
            tracing::warn!(user_id = %current.user_id, refresh_token_id = %current.id, "refresh token reuse detected; all sessions revoked");
        }
        return Err(unauthenticated("invalid refresh token"));
    }
    if current.expires_at <= now {
        return Err(unauthenticated("invalid refresh token"));
    }

    let active_username: Option<String> = sqlx::query_scalar(
        "SELECT username FROM users WHERE id = $1 AND deleted_at IS NULL AND suspended_at IS NULL",
    )
    .bind(current.user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;
    let username = active_username.ok_or_else(|| unauthenticated("invalid refresh token"))?;

    let (new_token, new_hash) =
        derive_refresh_token(current_token, rotation_request_id, rotation_key);
    let new_token_id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO refresh_tokens (
            id, user_id, token_hash, expires_at, user_agent, ip_address, device_id
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(new_token_id)
    .bind(current.user_id)
    .bind(new_hash)
    .bind(expires_at)
    .bind(user_agent)
    .bind(ip_address)
    .bind(current.device_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r#"
        UPDATE refresh_tokens
        SET revoked_at = $2,
            replaced_by_token_id = $3,
            last_used_at = $2,
            rotation_request_id = $4,
            rotated_at = $2
        WHERE id = $1
        "#,
    )
    .bind(current.id)
    .bind(now)
    .bind(new_token_id)
    .bind(rotation_request_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(RotatedRefreshToken {
        user_id: current.user_id,
        username,
        new_token_id,
        new_token,
    })
}

pub(crate) async fn revoke_refresh_token_by_hash(
    pool: &PgPool,
    token_hash: &[u8],
) -> Result<bool, ApiError> {
    let result = sqlx::query(
        r#"
        UPDATE refresh_tokens
        SET revoked_at = COALESCE(revoked_at, $2)
        WHERE token_hash = $1
        "#,
    )
    .bind(token_hash)
    .bind(OffsetDateTime::now_utc())
    .execute(pool)
    .await
    .map_err(internal_error)?;
    Ok(result.rows_affected() > 0)
}

pub(crate) async fn revoke_refresh_token_by_id_for_user(
    pool: &PgPool,
    user_id: Uuid,
    refresh_token_id: Uuid,
) -> Result<bool, ApiError> {
    let now = OffsetDateTime::now_utc();
    let result = sqlx::query(
        r#"
        UPDATE refresh_tokens
        SET revoked_at = COALESCE(revoked_at, $3)
        WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(refresh_token_id)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(internal_error)?;
    Ok(result.rows_affected() > 0)
}

pub(crate) async fn list_refresh_sessions(
    pool: &PgPool,
    user_id: Uuid,
    current_session_id: Uuid,
) -> Result<Vec<SessionSummary>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT id, device_id, id = $2 AS is_current, user_agent, ip_address,
               (extract(epoch FROM created_at) * 1000)::bigint AS created_at_ms,
               (extract(epoch FROM last_used_at) * 1000)::bigint AS last_used_at_ms,
               (extract(epoch FROM expires_at) * 1000)::bigint AS expires_at_ms
        FROM refresh_tokens
        WHERE user_id = $1
          AND revoked_at IS NULL
          AND expires_at > now()
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .bind(user_id)
    .bind(current_session_id)
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;
    rows.iter()
        .map(|row| {
            Ok(SessionSummary {
                refresh_token_id: row.try_get("id").map_err(internal_error)?,
                device_id: row.try_get("device_id").map_err(internal_error)?,
                is_current: row.try_get("is_current").map_err(internal_error)?,
                user_agent: row.try_get("user_agent").map_err(internal_error)?,
                ip_address: row.try_get("ip_address").map_err(internal_error)?,
                created_at_unix_ms: row.try_get("created_at_ms").map_err(internal_error)?,
                last_used_at_unix_ms: row.try_get("last_used_at_ms").map_err(internal_error)?,
                expires_at_unix_ms: row.try_get("expires_at_ms").map_err(internal_error)?,
            })
        })
        .collect()
}

pub(crate) async fn update_user_password_file_and_revoke_refresh_sessions(
    pool: &PgPool,
    user_id: Uuid,
    opaque_record: &[u8],
    encrypted_master_key: &[u8],
) -> Result<(), ApiError> {
    let now = OffsetDateTime::now_utc();
    let mut tx = pool.begin().await.map_err(internal_error)?;

    let updated =
        sqlx::query("UPDATE users SET opaque_record = $2, encrypted_master_key = $3 WHERE id = $1 AND deleted_at IS NULL AND suspended_at IS NULL")
            .bind(user_id)
            .bind(opaque_record)
            .bind(encrypted_master_key)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;
    if updated.rows_affected() == 0 {
        tx.rollback().await.map_err(internal_error)?;
        return Err(unauthenticated("user not found"));
    }

    sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = COALESCE(revoked_at, $2) WHERE user_id = $1",
    )
    .bind(user_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;
    Ok(())
}
