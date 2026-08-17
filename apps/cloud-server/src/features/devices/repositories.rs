//! Database access for devices.

use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::dto::{DevicePlatform, DeviceSummary, RegisterDeviceRequest};

pub(crate) async fn upsert_device(
    pool: &PgPool,
    user_id: Uuid,
    request: &RegisterDeviceRequest,
) -> anyhow::Result<DeviceSummary> {
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
    .fetch_one(pool)
    .await?;
    device_from_row(&row)
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
    let result = sqlx::query(
        r#"
        UPDATE devices
        SET status = 'revoked', revoked_at = now()
        WHERE id = $1 AND user_id = $2 AND status = 'active'
        "#,
    )
    .bind(device_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
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
