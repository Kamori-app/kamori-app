//! PostgreSQL-backed operator passkey lifecycle invariants.

use serde_json::Value;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use super::repositories::{
    RemoveSecurityKeyResult, RenamePasskeyResult, remove_security_key, rename_passkey,
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

async fn test_pool() -> Option<PgPool> {
    let database_url = std::env::var("KAMORI_DATABASE_URL")
        .ok()
        .or_else(|| std::env::var("DATABASE_URL").ok())?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect postgres");
    MIGRATOR.run(&pool).await.expect("run migrations");
    Some(pool)
}

#[tokio::test]
async fn passkey_names_are_unique_rename_is_audited_and_last_key_is_retained() {
    let Some(pool) = test_pool().await else {
        eprintln!("skipping admin integration test: DATABASE_URL is not set");
        return;
    };
    let admin_id = Uuid::new_v4();
    let primary_id = Uuid::new_v4();
    let backup_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO admin_users (id, username, totp_secret_ciphertext, status)
        VALUES ($1, $2, $3, 'active')
        "#,
    )
    .bind(admin_id)
    .bind(format!("passkey-test-{admin_id}"))
    .bind(vec![1_u8; 42])
    .execute(&pool)
    .await
    .expect("insert admin");
    for (key_id, name, credential_id) in [
        (primary_id, "Primary passkey", vec![2_u8; 32]),
        (backup_id, "Backup passkey", vec![3_u8; 32]),
    ] {
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
        .bind(credential_id)
        .bind(vec![4_u8])
        .execute(&pool)
        .await
        .expect("insert passkey");
    }

    let renamed = rename_passkey(
        &pool,
        admin_id,
        primary_id,
        "Bitwarden",
        "Make the provider recognizable",
        Some("127.0.0.1"),
    )
    .await
    .expect("rename passkey");
    assert!(matches!(renamed, RenamePasskeyResult::Renamed));
    let details: Value = sqlx::query_scalar(
        "SELECT details FROM admin_audit_log WHERE event_kind = 'operator_passkey_renamed' AND target_id = $1",
    )
    .bind(primary_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("load rename audit");
    assert_eq!(details["previous_name"], "Primary passkey");
    assert_eq!(details["new_name"], "Bitwarden");

    let conflict = rename_passkey(
        &pool,
        admin_id,
        primary_id,
        "BACKUP PASSKEY",
        "Exercise case-insensitive uniqueness",
        None,
    )
    .await
    .expect("detect duplicate name");
    assert!(matches!(conflict, RenamePasskeyResult::NameConflict));
    let duplicate_insert = sqlx::query(
        r#"
        INSERT INTO admin_security_keys (
            id, admin_user_id, name, credential_id, security_key_data
        ) VALUES ($1, $2, 'backup PASSKEY', $3, $4)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(admin_id)
    .bind(vec![5_u8; 32])
    .bind(vec![6_u8])
    .execute(&pool)
    .await;
    assert!(duplicate_insert.is_err());

    let removed = remove_security_key(
        &pool,
        admin_id,
        backup_id,
        "Retire the recovery passkey",
        None,
    )
    .await
    .expect("remove backup passkey");
    assert!(matches!(removed, RemoveSecurityKeyResult::Removed));
    let refused = remove_security_key(
        &pool,
        admin_id,
        primary_id,
        "Attempt to remove the final passkey",
        None,
    )
    .await
    .expect("retain final passkey");
    assert!(matches!(refused, RemoveSecurityKeyResult::WouldRemoveLast));
}
