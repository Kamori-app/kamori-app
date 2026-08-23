//! PostgreSQL-backed invariants for the signed operation log.

use crypto_core_lib::operation_envelope::{EnvelopeCipherSuite, EnvelopeKind, OperationEnvelopeV1};
use ed25519_dalek::SigningKey;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::features::ownership::{
    dto::OwnershipResourceKind,
    repositories::{AcceptOfferResult, CreateOfferResult, accept_offer, create_offer},
};

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

    let first_sequence =
        match append_operation(&pool, user_id, &envelope, 1_000_000_000, 1_000_000_000)
            .await
            .expect("append")
        {
            AppendResult::Accepted(sequence) => sequence,
            _ => panic!("first append must be accepted"),
        };
    match append_operation(&pool, user_id, &envelope, 1_000_000_000, 1_000_000_000)
        .await
        .expect("retry append")
    {
        AppendResult::Duplicate(sequence) => assert_eq!(sequence, first_sequence),
        _ => panic!("identical retry must be idempotent"),
    }
    let initial_space_bytes: i64 =
        sqlx::query_scalar("SELECT operation_bytes FROM security_spaces WHERE id = $1")
            .bind(space_id)
            .fetch_one(&pool)
            .await
            .expect("load initial space usage");
    let initial_account_bytes: i64 =
        sqlx::query_scalar("SELECT operation_bytes FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .expect("load initial account usage");
    assert!(initial_space_bytes > 0);
    assert_eq!(initial_account_bytes, initial_space_bytes);

    let mut conflicting_retry = envelope.clone();
    conflicting_retry.ciphertext = vec![99_u8];
    conflicting_retry.sign(&signing_key);
    assert!(matches!(
        append_operation(
            &pool,
            user_id,
            &conflicting_retry,
            1_000_000_000,
            1_000_000_000,
        )
        .await
        .expect("conflicting retry"),
        AppendResult::ConflictingDuplicate
    ));

    let mut concurrent_envelope = envelope.clone();
    concurrent_envelope.client_op_id = Uuid::new_v4();
    concurrent_envelope.ciphertext = vec![42_u8];
    concurrent_envelope.sign(&signing_key);
    let (left, right) = tokio::join!(
        append_operation(
            &pool,
            user_id,
            &concurrent_envelope,
            1_000_000_000,
            1_000_000_000
        ),
        append_operation(
            &pool,
            user_id,
            &concurrent_envelope,
            1_000_000_000,
            1_000_000_000
        ),
    );
    let concurrent_results = [
        left.expect("left concurrent append"),
        right.expect("right concurrent append"),
    ];
    assert_eq!(
        concurrent_results
            .iter()
            .filter(|result| matches!(result, AppendResult::Accepted(_)))
            .count(),
        1
    );
    assert_eq!(
        concurrent_results
            .iter()
            .filter(|result| matches!(result, AppendResult::Duplicate(_)))
            .count(),
        1
    );
    let last_sequence = concurrent_results
        .iter()
        .find_map(|result| match result {
            AppendResult::Accepted(sequence) | AppendResult::Duplicate(sequence) => Some(*sequence),
            _ => None,
        })
        .expect("concurrent sequence");
    let allocated_sequence: i64 =
        sqlx::query_scalar("SELECT next_sequence FROM security_spaces WHERE id = $1")
            .bind(space_id)
            .fetch_one(&pool)
            .await
            .expect("load allocated sequence");
    assert_eq!(u64::try_from(allocated_sequence).unwrap(), last_sequence);

    let current_space_bytes: i64 =
        sqlx::query_scalar("SELECT operation_bytes FROM security_spaces WHERE id = $1")
            .bind(space_id)
            .fetch_one(&pool)
            .await
            .expect("load space usage");
    let current_account_bytes: i64 =
        sqlx::query_scalar("SELECT operation_bytes FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .expect("load account usage");
    assert!(current_space_bytes > initial_space_bytes);
    assert_eq!(current_account_bytes, current_space_bytes);

    let mut over_quota = envelope.clone();
    over_quota.client_op_id = Uuid::new_v4();
    over_quota.ciphertext = vec![51_u8];
    over_quota.sign(&signing_key);
    assert!(matches!(
        append_operation(
            &pool,
            user_id,
            &over_quota,
            u64::try_from(current_space_bytes).unwrap(),
            u64::try_from(current_account_bytes).unwrap(),
        )
        .await
        .expect("quota rejection"),
        AppendResult::StorageQuotaExceeded
    ));
    let sequence_after_rejection: i64 =
        sqlx::query_scalar("SELECT next_sequence FROM security_spaces WHERE id = $1")
            .bind(space_id)
            .fetch_one(&pool)
            .await
            .expect("load sequence after quota rejection");
    assert_eq!(sequence_after_rejection, allocated_sequence);

    let listed = list_operations(&pool, user_id, space_id, 0, 100, 8 * 1024 * 1024)
        .await
        .expect("list operations")
        .expect("read access");
    assert_eq!(listed.effective_since, 0);
    assert_eq!(listed.operations.len(), 2);
    assert_eq!(listed.operations[0].envelope, envelope);
    let byte_bounded = list_operations(&pool, user_id, space_id, 0, 100, 1)
        .await
        .expect("list byte-bounded operations")
        .expect("read access");
    assert_eq!(byte_bounded.operations.len(), 1);

    sqlx::query(
        "UPDATE security_space_members SET history_start_seq = $3 WHERE space_id = $1 AND user_id = $2",
    )
    .bind(space_id)
    .bind(user_id)
    .bind(i64::try_from(last_sequence).unwrap())
    .execute(&pool)
    .await
    .expect("advance membership history boundary");
    let bounded = list_operations(&pool, user_id, space_id, 0, 100, 8 * 1024 * 1024)
        .await
        .expect("list bounded operations")
        .expect("bounded read access");
    assert_eq!(bounded.effective_since, last_sequence);
    assert!(bounded.operations.is_empty());

    let recipient_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO users (
            id, username, opaque_record, encrypted_master_key,
            public_key_bundle, recovery_verifier_hash
        ) VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(recipient_id)
    .bind(format!("oplog-recipient-{recipient_id}"))
    .bind(vec![1_u8])
    .bind(vec![2_u8; 49])
    .bind(vec![3_u8])
    .bind(vec![8_u8; 32])
    .execute(&pool)
    .await
    .expect("insert ownership recipient");
    sqlx::query(
        "INSERT INTO security_space_members (id, space_id, user_id, role, key_epoch) VALUES ($1, $2, $3, 'editor', 1)",
    )
    .bind(Uuid::new_v4())
    .bind(space_id)
    .bind(recipient_id)
    .execute(&pool)
    .await
    .expect("insert recipient membership");
    let blob_id = Uuid::new_v4();
    let transferred_blob_bytes = 512_i64;
    sqlx::query(
        r#"
        INSERT INTO space_blobs (
            id, space_id, owner_user_id, created_by, ciphertext_sha256,
            size_padded, object_key, status
        ) VALUES ($1, $2, $3, $3, $4, $5, $6, 'ready')
        "#,
    )
    .bind(blob_id)
    .bind(space_id)
    .bind(user_id)
    .bind(vec![31_u8; 32])
    .bind(transferred_blob_bytes)
    .bind(format!("test/{space_id}/{blob_id}"))
    .execute(&pool)
    .await
    .expect("insert transferred blob");
    let transfer_id = match create_offer(
        &pool,
        user_id,
        OwnershipResourceKind::SecuritySpace,
        space_id,
        recipient_id,
    )
    .await
    .expect("create ownership offer")
    {
        CreateOfferResult::Created(offer) => offer.transfer_id,
        _ => panic!("ownership offer must be created"),
    };
    assert!(matches!(
        accept_offer(
            &pool,
            recipient_id,
            transfer_id,
            transferred_blob_bytes - 1,
            i64::MAX,
        )
        .await
        .expect("reject blob over-quota ownership transfer"),
        AcceptOfferResult::BlobStorageQuotaExceeded
    ));
    assert!(matches!(
        accept_offer(
            &pool,
            recipient_id,
            transfer_id,
            i64::MAX,
            current_space_bytes - 1,
        )
        .await
        .expect("reject over-quota ownership transfer"),
        AcceptOfferResult::OperationStorageQuotaExceeded
    ));
    assert!(matches!(
        accept_offer(&pool, recipient_id, transfer_id, i64::MAX, i64::MAX)
            .await
            .expect("accept ownership transfer"),
        AcceptOfferResult::Accepted
    ));
    let transferred_owner: Uuid =
        sqlx::query_scalar("SELECT owner_user_id FROM security_spaces WHERE id = $1")
            .bind(space_id)
            .fetch_one(&pool)
            .await
            .expect("load transferred owner");
    assert_eq!(transferred_owner, recipient_id);
    let former_owner_bytes: i64 =
        sqlx::query_scalar("SELECT operation_bytes FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .expect("load former owner operation usage");
    let recipient_bytes: i64 =
        sqlx::query_scalar("SELECT operation_bytes FROM users WHERE id = $1")
            .bind(recipient_id)
            .fetch_one(&pool)
            .await
            .expect("load recipient operation usage");
    assert_eq!(former_owner_bytes, 0);
    assert_eq!(recipient_bytes, current_space_bytes);
    let former_owner_blob_bytes: i64 =
        sqlx::query_scalar("SELECT blob_storage_bytes FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .expect("load former owner blob usage");
    let recipient_blob_bytes: i64 =
        sqlx::query_scalar("SELECT blob_storage_bytes FROM users WHERE id = $1")
            .bind(recipient_id)
            .fetch_one(&pool)
            .await
            .expect("load recipient blob usage");
    let blob_owner: Uuid =
        sqlx::query_scalar("SELECT owner_user_id FROM space_blobs WHERE space_id = $1 AND id = $2")
            .bind(space_id)
            .bind(blob_id)
            .fetch_one(&pool)
            .await
            .expect("load transferred blob owner");
    assert_eq!(former_owner_blob_bytes, 0);
    assert_eq!(recipient_blob_bytes, transferred_blob_bytes);
    assert_eq!(blob_owner, recipient_id);

    pool.close().await;
}
