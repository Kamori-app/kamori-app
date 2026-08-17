//! PostgreSQL-backed invariants for the signed operation log.

use crypto_core_lib::operation_envelope::{EnvelopeCipherSuite, EnvelopeKind, OperationEnvelopeV1};
use ed25519_dalek::SigningKey;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use super::repositories::{
    AppendResult, append_operation, list_operations, load_append_authorization,
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
async fn append_is_signed_authorized_and_idempotent() {
    let Some(pool) = test_pool().await else {
        eprintln!("skipping operation integration test: DATABASE_URL is not set");
        return;
    };

    let user_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let space_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let signing_key = SigningKey::from_bytes(&[17; 32]);

    sqlx::query(
        r#"
        INSERT INTO users (
            id, username, opaque_record, encrypted_master_key,
            public_key_bundle, recovery_verifier_hash
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(user_id)
    .bind(format!("oplog-{user_id}"))
    .bind(vec![1_u8])
    .bind(vec![2_u8; 49])
    .bind(vec![3_u8])
    .bind(vec![7_u8; 32])
    .execute(&pool)
    .await
    .expect("insert user");
    sqlx::query(
        "INSERT INTO workspaces (id, owner_user_id, kind, encrypted_metadata) VALUES ($1, $2, 'personal', $3)",
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(vec![4_u8])
    .execute(&pool)
    .await
    .expect("insert workspace");
    sqlx::query(
        "INSERT INTO devices (id, user_id, signing_public_key, hpke_public_key, encrypted_name, platform) VALUES ($1, $2, $3, $4, $5, 'web')",
    )
    .bind(device_id)
    .bind(user_id)
    .bind(signing_key.verifying_key().as_bytes().as_slice())
    .bind(vec![5_u8; 32])
    .bind(vec![6_u8])
    .execute(&pool)
    .await
    .expect("insert device");
    sqlx::query(
        "INSERT INTO security_spaces (id, workspace_id, owner_user_id, created_by, encrypted_metadata) VALUES ($1, $2, $3, $3, $4)",
    )
    .bind(space_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(vec![7_u8])
    .execute(&pool)
    .await
    .expect("insert space");
    sqlx::query(
        "INSERT INTO security_space_members (id, space_id, user_id, role, key_epoch) VALUES ($1, $2, $3, 'owner', 1)",
    )
    .bind(Uuid::new_v4())
    .bind(space_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("insert space membership");

    let mut envelope = OperationEnvelopeV1 {
        space_id,
        stream_id: Uuid::new_v4(),
        client_op_id: Uuid::new_v4(),
        author_device_id: device_id,
        key_epoch: 1,
        envelope_kind: EnvelopeKind::Operation,
        cipher_suite: EnvelopeCipherSuite::Xchacha20Poly1305,
        nonce: vec![9; 24],
        ciphertext: vec![10, 11],
        signature: Vec::new(),
    };
    envelope.sign(&signing_key);

    let authorization = load_append_authorization(&pool, user_id, &envelope)
        .await
        .expect("load authorization")
        .expect("authorized");
    assert!(authorization.can_write);
    envelope
        .verify(&authorization.signing_public_key)
        .expect("valid device signature");

    let first_sequence = match append_operation(&pool, user_id, &envelope)
        .await
        .expect("append")
    {
        AppendResult::Accepted(sequence) => sequence,
        _ => panic!("first append must be accepted"),
    };
    match append_operation(&pool, user_id, &envelope)
        .await
        .expect("retry append")
    {
        AppendResult::Duplicate(sequence) => assert_eq!(sequence, first_sequence),
        _ => panic!("identical retry must be idempotent"),
    }

    let listed = list_operations(&pool, user_id, space_id, 0, 100)
        .await
        .expect("list operations")
        .expect("read access");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].envelope, envelope);

    pool.close().await;
}
