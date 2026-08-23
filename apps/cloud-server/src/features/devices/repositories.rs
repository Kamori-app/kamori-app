//! Database access for devices.

use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::dto::{DevicePlatform, DeviceSummary, RegisterDeviceRequest};

pub(crate) async fn upsert_device(
    pool: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
    request: &RegisterDeviceRequest,
) -> anyhow::Result<Option<DeviceSummary>> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        r#"
        INSERT INTO devices (
            id, user_id, signing_public_key, hpke_public_key, encrypted_name, platform
        ) VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (id) DO UPDATE SET
            encrypted_name = EXCLUDED.encrypted_name,
            platform = EXCLUDED.platform,
            last_seen_at = now()
        WHERE devices.user_id = EXCLUDED.user_id
          AND devices.status = 'active'
          AND devices.signing_public_key = EXCLUDED.signing_public_key
          AND devices.hpke_public_key = EXCLUDED.hpke_public_key
        RETURNING id, signing_public_key, hpke_public_key, encrypted_name, platform,
                  (extract(epoch FROM created_at) * 1000)::bigint AS created_at_ms,
                  (extract(epoch FROM last_seen_at) * 1000)::bigint AS last_seen_at_ms
        "#,
    )
    .bind(request.device_id)
    .bind(user_id)
    .bind(&request.signing_public_key)
    .bind(&request.hpke_public_key)
    .bind(&request.encrypted_name)
    .bind(request.platform.as_db_value())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let bound = sqlx::query(
        r#"
        UPDATE refresh_tokens
        SET device_id = $3
        WHERE id = $1 AND user_id = $2
          AND revoked_at IS NULL AND expires_at > now()
          AND (device_id IS NULL OR device_id = $3)
        "#,
    )
    .bind(session_id)
    .bind(user_id)
    .bind(request.device_id)
    .execute(&mut *tx)
    .await?;
    if bound.rows_affected() != 1 {
        return Ok(None);
    }
    tx.commit().await?;
    device_from_row(&row).map(Some)
}

pub(crate) async fn list_devices(
    pool: &PgPool,
    user_id: Uuid,
) -> anyhow::Result<Vec<DeviceSummary>> {
    let rows = sqlx::query(
        r#"
        SELECT id, signing_public_key, hpke_public_key, encrypted_name, platform,
               (extract(epoch FROM created_at) * 1000)::bigint AS created_at_ms,
               (extract(epoch FROM last_seen_at) * 1000)::bigint AS last_seen_at_ms
        FROM devices
        WHERE user_id = $1 AND status = 'active'
        ORDER BY created_at ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    rows.iter().map(device_from_row).collect()
}

pub(crate) async fn revoke_device(
    pool: &PgPool,
    user_id: Uuid,
    device_id: Uuid,
) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        r#"
        UPDATE devices
        SET status = 'revoked', revoked_at = now()
        WHERE id = $1 AND user_id = $2 AND status = 'active'
        "#,
    )
    .bind(device_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() != 1 {
        return Ok(false);
    }
    let now = time::OffsetDateTime::now_utc();
    sqlx::query(
        r#"
        UPDATE refresh_tokens
        SET revoked_at = COALESCE(revoked_at, $3)
        WHERE user_id = $1 AND device_id = $2
        "#,
    )
    .bind(user_id)
    .bind(device_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM security_space_device_keys WHERE user_id = $1 AND device_id = $2")
        .bind(user_id)
        .bind(device_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        r#"
        INSERT INTO security_events (id, user_id, event_kind, details)
        VALUES ($1, $2, 'device_revoked', jsonb_build_object('device_id', $3::text))
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(device_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

fn device_from_row(row: &sqlx::postgres::PgRow) -> anyhow::Result<DeviceSummary> {
    let platform: String = row.try_get("platform")?;
    let platform = match platform.as_str() {
        "web" => DevicePlatform::Web,
        "desktop" => DevicePlatform::Desktop,
        "android" => DevicePlatform::Android,
        "ios" => DevicePlatform::Ios,
        _ => anyhow::bail!("unknown device platform"),
    };
    Ok(DeviceSummary {
        device_id: row.try_get("id")?,
        signing_public_key: row.try_get("signing_public_key")?,
        hpke_public_key: row.try_get("hpke_public_key")?,
        encrypted_name: row.try_get("encrypted_name")?,
        platform,
        created_at_unix_ms: row.try_get("created_at_ms")?,
        last_seen_at_unix_ms: row.try_get("last_seen_at_ms")?,
    })
}
