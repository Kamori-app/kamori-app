//! Persistence for isolated operator identities, sessions, audit, and controls.

use std::collections::HashMap;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;
use webauthn_rs::prelude::{AuthenticationResult, SecurityKey};

use crate::{
    features::admin::dto::{AdminAuditEntry, AdminSecurityKeySummary, OperatorJobStatus},
    platform::security::passkey::{decode_security_key, encode_security_key},
};

#[derive(Debug)]
pub(crate) struct AdminIdentity {
    pub(crate) id: Uuid,
    pub(crate) username: String,
    pub(crate) totp_secret_ciphertext: Vec<u8>,
    pub(crate) security_keys: Vec<SecurityKey>,
}

#[derive(Debug)]
pub(crate) struct IssuedAdminToken {
    pub(crate) token: String,
    pub(crate) expires_at: OffsetDateTime,
}

#[derive(Debug)]
pub(crate) struct BootstrapCredentials {
    pub(crate) admin_user_id: Uuid,
    pub(crate) token: String,
}

#[derive(Debug)]
pub(crate) struct DashboardCounts {
    pub(crate) active_accounts: i64,
    pub(crate) suspended_accounts: i64,
    pub(crate) total_blob_storage_bytes: i64,
    pub(crate) pending_blobs: i64,
    pub(crate) pending_object_deletions: i64,
    pub(crate) latest_migration: Option<String>,
    pub(crate) jobs: Vec<OperatorJobStatus>,
    pub(crate) security_keys: Vec<AdminSecurityKeySummary>,
}

#[derive(Debug)]
pub(crate) struct StoredRuntimeSetting {
    pub(crate) key: String,
    pub(crate) value: Value,
    pub(crate) version: i64,
    pub(crate) updated_at: OffsetDateTime,
}

pub(crate) enum RemoveSecurityKeyResult {
    Removed,
    NotFound,
    WouldViolateMinimum { required: i64 },
}

pub(crate) enum SetRuntimeValueResult {
    Changed,
    VersionConflict,
    SecurityKeyMinimum,
}

fn new_secret_token(prefix: &str) -> (String, Vec<u8>) {
    let mut raw = [0_u8; 32];
    rand::rng().fill(&mut raw);
    let token = format!("{prefix}{}", URL_SAFE_NO_PAD.encode(raw));
    let hash = Sha256::digest(token.as_bytes()).to_vec();
    (token, hash)
}

fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub(crate) async fn create_bootstrap(
    pool: &PgPool,
    username: &str,
    totp_secret_ciphertext: &[u8],
) -> anyhow::Result<BootstrapCredentials> {
    let mut tx = pool.begin().await?;
    let admin_user_id = Uuid::new_v4();
    let row = sqlx::query(
        r#"
        INSERT INTO admin_users (id, username, totp_secret_ciphertext)
        VALUES ($1, $2, $3)
        ON CONFLICT (username) DO UPDATE SET
            totp_secret_ciphertext = CASE WHEN admin_users.status = 'pending'
                               THEN EXCLUDED.totp_secret_ciphertext
                               ELSE admin_users.totp_secret_ciphertext END
        RETURNING id, status
        "#,
    )
    .bind(admin_user_id)
    .bind(username)
    .bind(totp_secret_ciphertext)
    .fetch_one(&mut *tx)
    .await?;
    let status: String = row.try_get("status")?;
    anyhow::ensure!(
        status == "pending",
        "operator already exists and is not pending; bootstrap refused"
    );
    let admin_user_id: Uuid = row.try_get("id")?;
    sqlx::query(
        "UPDATE admin_bootstrap_tokens SET used_at = now() WHERE admin_user_id = $1 AND used_at IS NULL",
    )
    .bind(admin_user_id)
    .execute(&mut *tx)
    .await?;
    let (token, hash) = new_secret_token("kamori_bootstrap_");
    sqlx::query(
        r#"
        INSERT INTO admin_bootstrap_tokens (id, admin_user_id, token_hash, expires_at)
        VALUES ($1, $2, $3, now() + interval '15 minutes')
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(admin_user_id)
    .bind(hash)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO admin_audit_log (id, actor_admin_user_id, event_kind, target_kind, target_id) VALUES ($1, NULL, 'operator_bootstrap_created', 'admin_user', $2)",
    )
    .bind(Uuid::new_v4())
    .bind(admin_user_id.to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(BootstrapCredentials {
        admin_user_id,
        token,
    })
}

pub(crate) async fn validate_bootstrap(
    pool: &PgPool,
    username: &str,
    token: &str,
) -> anyhow::Result<Option<AdminIdentity>> {
    let row = sqlx::query(
        r#"
        SELECT admin.id, admin.username, admin.totp_secret_ciphertext
        FROM admin_bootstrap_tokens bootstrap
        JOIN admin_users admin ON admin.id = bootstrap.admin_user_id
        WHERE bootstrap.token_hash = $1 AND bootstrap.used_at IS NULL
          AND bootstrap.expires_at > now() AND admin.username = $2
          AND admin.status = 'pending'
        "#,
    )
    .bind(token_hash(token))
    .bind(username)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        Ok(AdminIdentity {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            totp_secret_ciphertext: row.try_get("totp_secret_ciphertext")?,
            security_keys: Vec::new(),
        })
    })
    .transpose()
}

pub(crate) async fn activate_with_security_key(
    pool: &PgPool,
    admin_id: Uuid,
    bootstrap_token: &str,
    key: &SecurityKey,
) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;
    let bootstrap_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id FROM admin_bootstrap_tokens
        WHERE admin_user_id = $1 AND token_hash = $2
          AND used_at IS NULL AND expires_at > now()
        FOR UPDATE
        "#,
    )
    .bind(admin_id)
    .bind(token_hash(bootstrap_token))
    .fetch_optional(&mut *tx)
    .await?;
    let Some(bootstrap_id) = bootstrap_id else {
        tx.rollback().await?;
        return Ok(false);
    };
    sqlx::query(
        r#"
        INSERT INTO admin_security_keys (id, admin_user_id, name, credential_id, security_key_data)
        VALUES ($1, $2, 'Primary security key', $3, $4)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(admin_id)
    .bind(key.cred_id().as_ref())
    .bind(encode_security_key(key)?)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE admin_bootstrap_tokens SET used_at = now() WHERE id = $1")
        .bind(bootstrap_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE admin_users SET status = 'active', activated_at = now() WHERE id = $1 AND status = 'pending'",
    )
    .bind(admin_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO admin_audit_log (id, actor_admin_user_id, event_kind, target_kind, target_id) VALUES ($1, $2, 'operator_activated', 'admin_user', $3)",
    )
    .bind(Uuid::new_v4())
    .bind(admin_id)
    .bind(admin_id.to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

pub(crate) async fn load_active_identity(
    pool: &PgPool,
    username: &str,
) -> anyhow::Result<Option<AdminIdentity>> {
    let row = sqlx::query(
        "SELECT id, username, totp_secret_ciphertext FROM admin_users WHERE username = $1 AND status = 'active'",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let id: Uuid = row.try_get("id")?;
    let key_rows = sqlx::query(
        "SELECT security_key_data FROM admin_security_keys WHERE admin_user_id = $1 ORDER BY created_at",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let security_keys = key_rows
        .iter()
        .map(|row| decode_security_key(&row.try_get::<Vec<u8>, _>("security_key_data")?))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(Some(AdminIdentity {
        id,
        username: row.try_get("username")?,
        totp_secret_ciphertext: row.try_get("totp_secret_ciphertext")?,
        security_keys,
    }))
}

pub(crate) async fn persist_security_key_result(
    pool: &PgPool,
    admin_id: Uuid,
    result: &AuthenticationResult,
) -> anyhow::Result<()> {
    let row = sqlx::query(
        "SELECT security_key_data FROM admin_security_keys WHERE admin_user_id = $1 AND credential_id = $2",
    )
    .bind(admin_id)
    .bind(result.cred_id().as_ref())
    .fetch_one(pool)
    .await?;
    let mut key = decode_security_key(&row.try_get::<Vec<u8>, _>("security_key_data")?)?;
    let changed = key
        .update_credential(result)
        .ok_or_else(|| anyhow::anyhow!("operator credential mismatch"))?;
    sqlx::query(
        r#"
        UPDATE admin_security_keys
        SET security_key_data = CASE WHEN $3 THEN $4 ELSE security_key_data END,
            last_used_at = now()
        WHERE admin_user_id = $1 AND credential_id = $2
        "#,
    )
    .bind(admin_id)
    .bind(result.cred_id().as_ref())
    .bind(changed)
    .bind(encode_security_key(&key)?)
    .execute(pool)
    .await?;
    sqlx::query("UPDATE admin_users SET last_login_at = now() WHERE id = $1")
        .bind(admin_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn add_security_key(
    pool: &PgPool,
    admin_id: Uuid,
    name: &str,
    key: &SecurityKey,
    reason: &str,
    ip_address: Option<&str>,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let key_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO admin_security_keys (
            id, admin_user_id, name, credential_id, security_key_data
        ) VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(key_id)
    .bind(admin_id)
    .bind(name)
    .bind(key.cred_id().as_ref())
    .bind(encode_security_key(key)?)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO admin_audit_log (
            id, actor_admin_user_id, event_kind, target_kind, target_id, reason, ip_address
        ) VALUES ($1, $2, 'operator_security_key_added', 'admin_security_key', $3, $4, $5)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(admin_id)
    .bind(key_id.to_string())
    .bind(reason)
    .bind(ip_address)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn remove_security_key(
    pool: &PgPool,
    admin_id: Uuid,
    key_id: Uuid,
    reason: &str,
    ip_address: Option<&str>,
    registration_enabled_default: bool,
) -> anyhow::Result<RemoveSecurityKeyResult> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT id FROM admin_users WHERE id = $1 FOR UPDATE")
        .bind(admin_id)
        .fetch_one(&mut *tx)
        .await?;
    let registration_enabled = sqlx::query_scalar::<_, Value>(
        "SELECT value FROM runtime_config_overrides WHERE key = 'registration_enabled'",
    )
    .fetch_optional(&mut *tx)
    .await?
    .and_then(|value| value.as_bool())
    .unwrap_or(registration_enabled_default);
    let required = if registration_enabled { 2 } else { 1 };
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM admin_security_keys WHERE admin_user_id = $1",
    )
    .bind(admin_id)
    .fetch_one(&mut *tx)
    .await?;
    if count <= required {
        tx.rollback().await?;
        return Ok(RemoveSecurityKeyResult::WouldViolateMinimum { required });
    }
    let removed =
        sqlx::query("DELETE FROM admin_security_keys WHERE id = $1 AND admin_user_id = $2")
            .bind(key_id)
            .bind(admin_id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
    if !removed {
        tx.rollback().await?;
        return Ok(RemoveSecurityKeyResult::NotFound);
    }
    sqlx::query(
        "UPDATE admin_sessions SET revoked_at = COALESCE(revoked_at, now()) WHERE admin_user_id = $1",
    )
    .bind(admin_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO admin_audit_log (
            id, actor_admin_user_id, event_kind, target_kind, target_id, reason, ip_address
        ) VALUES ($1, $2, 'operator_security_key_removed', 'admin_security_key', $3, $4, $5)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(admin_id)
    .bind(key_id.to_string())
    .bind(reason)
    .bind(ip_address)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(RemoveSecurityKeyResult::Removed)
}

pub(crate) async fn issue_token(
    pool: &PgPool,
    admin_id: Uuid,
    kind: &str,
    ttl: Duration,
    user_agent: Option<&str>,
    ip_address: Option<&str>,
) -> anyhow::Result<IssuedAdminToken> {
    let (token, hash) = new_secret_token("kamori_admin_");
    let expires_at = OffsetDateTime::now_utc() + ttl;
    sqlx::query(
        r#"
        INSERT INTO admin_sessions (
            id, admin_user_id, token_hash, session_kind, expires_at, user_agent, ip_address
        ) VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(admin_id)
    .bind(hash)
    .bind(kind)
    .bind(expires_at)
    .bind(user_agent)
    .bind(ip_address)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO admin_audit_log (id, actor_admin_user_id, event_kind, target_kind, target_id) VALUES ($1, $2, $3, 'admin_user', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(admin_id)
    .bind(if kind == "reauth" {
        "operator_reauthenticated"
    } else {
        "operator_login"
    })
    .bind(admin_id.to_string())
    .execute(pool)
    .await?;
    Ok(IssuedAdminToken { token, expires_at })
}

pub(crate) async fn revoke_session_token(pool: &PgPool, token: &str) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE admin_sessions SET revoked_at = now() WHERE token_hash = $1 AND session_kind = 'session' AND revoked_at IS NULL",
    )
    .bind(token_hash(token))
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn authorize_token(
    pool: &PgPool,
    token: &str,
    expected_kind: &str,
) -> anyhow::Result<Option<AdminIdentity>> {
    let row = sqlx::query(
        r#"
        UPDATE admin_sessions session
        SET last_used_at = now()
        FROM admin_users admin
        WHERE session.token_hash = $1 AND session.session_kind = $2
          AND session.revoked_at IS NULL AND session.expires_at > now()
          AND admin.id = session.admin_user_id AND admin.status = 'active'
        RETURNING admin.id, admin.username, admin.totp_secret_ciphertext
        "#,
    )
    .bind(token_hash(token))
    .bind(expected_kind)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        Ok(AdminIdentity {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            totp_secret_ciphertext: row.try_get("totp_secret_ciphertext")?,
            security_keys: Vec::new(),
        })
    })
    .transpose()
}

pub(crate) async fn consume_reauth_token(
    pool: &PgPool,
    token: &str,
    actor_id: Uuid,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE admin_sessions
        SET revoked_at = now(), last_used_at = now()
        WHERE token_hash = $1 AND admin_user_id = $2 AND session_kind = 'reauth'
          AND revoked_at IS NULL AND expires_at > now()
        "#,
    )
    .bind(token_hash(token))
    .bind(actor_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn dashboard_counts(
    pool: &PgPool,
    admin_id: Uuid,
) -> anyhow::Result<DashboardCounts> {
    let row = sqlx::query(
        r#"
        SELECT
          (SELECT count(*)::bigint FROM users WHERE deleted_at IS NULL AND suspended_at IS NULL) AS active_accounts,
          (SELECT count(*)::bigint FROM users WHERE deleted_at IS NULL AND suspended_at IS NOT NULL) AS suspended_accounts,
          (SELECT COALESCE(sum(size_padded), 0)::bigint FROM space_blobs WHERE status = 'ready') AS storage_bytes,
          (SELECT count(*)::bigint FROM space_blobs WHERE status = 'pending') AS pending_blobs,
          (SELECT count(*)::bigint FROM object_deletion_queue) AS pending_deletions,
          (SELECT version::text FROM _sqlx_migrations WHERE success ORDER BY version DESC LIMIT 1) AS latest_migration
        "#,
    )
    .fetch_one(pool)
    .await?;
    let job_rows = sqlx::query(
        r#"
        SELECT job_name, status, details,
               (extract(epoch FROM updated_at) * 1000)::bigint AS updated_at_ms,
               (extract(epoch FROM last_succeeded_at) * 1000)::bigint AS succeeded_at_ms
        FROM operator_job_heartbeats ORDER BY job_name
        "#,
    )
    .fetch_all(pool)
    .await?;
    let jobs = job_rows
        .iter()
        .map(|row| {
            Ok(OperatorJobStatus {
                job_name: row.try_get("job_name")?,
                status: row.try_get("status")?,
                details: row.try_get("details")?,
                updated_at_unix_ms: row.try_get("updated_at_ms")?,
                last_succeeded_at_unix_ms: row.try_get("succeeded_at_ms")?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let security_key_rows = sqlx::query(
        r#"
        SELECT key.id, key.name,
               (extract(epoch FROM key.created_at) * 1000)::bigint AS created_at_ms,
               (extract(epoch FROM key.last_used_at) * 1000)::bigint AS last_used_at_ms
        FROM admin_security_keys key
        JOIN admin_users admin ON admin.id = key.admin_user_id
        WHERE admin.status = 'active' AND admin.id = $1
        ORDER BY key.created_at
        "#,
    )
    .bind(admin_id)
    .fetch_all(pool)
    .await?;
    let security_keys = security_key_rows
        .iter()
        .map(|row| {
            Ok(AdminSecurityKeySummary {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                created_at_unix_ms: row.try_get("created_at_ms")?,
                last_used_at_unix_ms: row.try_get("last_used_at_ms")?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(DashboardCounts {
        active_accounts: row.try_get("active_accounts")?,
        suspended_accounts: row.try_get("suspended_accounts")?,
        total_blob_storage_bytes: row.try_get("storage_bytes")?,
        pending_blobs: row.try_get("pending_blobs")?,
        pending_object_deletions: row.try_get("pending_deletions")?,
        latest_migration: row.try_get("latest_migration")?,
        jobs,
        security_keys,
    })
}

pub(crate) async fn list_runtime_settings(
    pool: &PgPool,
) -> anyhow::Result<Vec<StoredRuntimeSetting>> {
    let rows = sqlx::query(
        "SELECT key, value, version, updated_at FROM runtime_config_overrides ORDER BY key",
    )
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(StoredRuntimeSetting {
                key: row.try_get("key")?,
                value: row.try_get("value")?,
                version: row.try_get("version")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .collect()
}

pub(crate) async fn get_runtime_value(pool: &PgPool, key: &str) -> anyhow::Result<Option<Value>> {
    sqlx::query_scalar("SELECT value FROM runtime_config_overrides WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn get_runtime_values(
    pool: &PgPool,
    keys: &[String],
) -> anyhow::Result<HashMap<String, Value>> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query("SELECT key, value FROM runtime_config_overrides WHERE key = ANY($1)")
        .bind(keys)
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|row| Ok((row.try_get("key")?, row.try_get("value")?)))
        .collect()
}

pub(crate) async fn set_runtime_value(
    pool: &PgPool,
    actor_id: Uuid,
    key: &str,
    value: &Value,
    expected_version: i64,
    reason: &str,
    ip_address: Option<&str>,
) -> anyhow::Result<SetRuntimeValueResult> {
    let mut tx = pool.begin().await?;
    if key == "registration_enabled" && value == &Value::Bool(true) {
        sqlx::query("SELECT id FROM admin_users WHERE id = $1 FOR UPDATE")
            .bind(actor_id)
            .fetch_one(&mut *tx)
            .await?;
        let key_count: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM admin_security_keys WHERE admin_user_id = $1",
        )
        .bind(actor_id)
        .fetch_one(&mut *tx)
        .await?;
        if key_count < 2 {
            tx.rollback().await?;
            return Ok(SetRuntimeValueResult::SecurityKeyMinimum);
        }
    }
    let changed = if expected_version == 0 {
        sqlx::query(
            r#"
            INSERT INTO runtime_config_overrides (key, value, updated_by)
            VALUES ($1, $2, $3) ON CONFLICT (key) DO NOTHING
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(actor_id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1
    } else {
        sqlx::query(
            r#"
            UPDATE runtime_config_overrides
            SET value = $2, version = version + 1, updated_by = $3, updated_at = now()
            WHERE key = $1 AND version = $4
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(actor_id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1
    };
    if changed {
        sqlx::query(
            r#"
            INSERT INTO admin_audit_log (
                id, actor_admin_user_id, event_kind, target_kind, target_id, reason, details, ip_address
            ) VALUES ($1, $2, 'runtime_setting_updated', 'runtime_setting', $3, $4,
                      jsonb_build_object('value', $5::jsonb, 'expected_version', $6::bigint), $7)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(key)
        .bind(reason)
        .bind(value)
        .bind(expected_version)
        .bind(ip_address)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(if changed {
        SetRuntimeValueResult::Changed
    } else {
        SetRuntimeValueResult::VersionConflict
    })
}

pub(crate) async fn suspend_account(
    pool: &PgPool,
    actor_id: Uuid,
    user_id: Uuid,
    suspended: bool,
    reason: &str,
    ip_address: Option<&str>,
) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        r#"
        UPDATE users SET
            suspended_at = CASE WHEN $2 THEN now() ELSE NULL END,
            suspension_reason = CASE WHEN $2 THEN $3 ELSE NULL END
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(user_id)
    .bind(suspended)
    .bind(reason)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 1 {
        if suspended {
            sqlx::query(
                "UPDATE refresh_tokens SET revoked_at = COALESCE(revoked_at, now()) WHERE user_id = $1",
            )
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            r#"
            INSERT INTO admin_audit_log (
                id, actor_admin_user_id, event_kind, target_kind, target_id, reason, details, ip_address
            ) VALUES ($1, $2, $3, 'user', $4, $5, $6, $7)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(if suspended { "account_suspended" } else { "account_unsuspended" })
        .bind(user_id.to_string())
        .bind(reason)
        .bind(json!({ "suspended": suspended }))
        .bind(ip_address)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn list_audit(pool: &PgPool, limit: i64) -> anyhow::Result<Vec<AdminAuditEntry>> {
    let rows = sqlx::query(
        r#"
        SELECT audit.id, actor.username AS actor_username, audit.event_kind,
               audit.target_kind, audit.target_id, audit.reason, audit.details,
               (extract(epoch FROM audit.created_at) * 1000)::bigint AS created_at_ms
        FROM admin_audit_log audit
        LEFT JOIN admin_users actor ON actor.id = audit.actor_admin_user_id
        ORDER BY audit.created_at DESC LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(AdminAuditEntry {
                id: row.try_get("id")?,
                actor_username: row.try_get("actor_username")?,
                event_kind: row.try_get("event_kind")?,
                target_kind: row.try_get("target_kind")?,
                target_id: row.try_get("target_id")?,
                reason: row.try_get("reason")?,
                details: row.try_get("details")?,
                created_at_unix_ms: row.try_get("created_at_ms")?,
            })
        })
        .collect()
}
