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
    let (_, envelope) = operation_fixture(&pool, user_id).await;
    let space_id = envelope.space_id;
    let signing_key = SigningKey::from_bytes(&[17; 32]);

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

async fn operation_fixture(pool: &PgPool, user_id: Uuid) -> (Uuid, OperationEnvelopeV1) {
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
    .execute(pool)
    .await
    .expect("insert user");
    sqlx::query(
        "INSERT INTO workspaces (id, owner_user_id, kind, encrypted_metadata) VALUES ($1, $2, 'personal', $3)",
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(vec![4_u8])
    .execute(pool)
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
    .execute(pool)
    .await
    .expect("insert device");
    sqlx::query(
        "INSERT INTO security_spaces (id, workspace_id, owner_user_id, created_by, encrypted_metadata) VALUES ($1, $2, $3, $3, $4)",
    )
    .bind(space_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(vec![7_u8])
    .execute(pool)
    .await
    .expect("insert space");
    sqlx::query(
        "INSERT INTO security_space_members (id, space_id, user_id, role, key_epoch) VALUES ($1, $2, $3, 'owner', 1)",
    )
    .bind(Uuid::new_v4())
    .bind(space_id)
    .bind(user_id)
    .execute(pool)
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

    (workspace_id, envelope)
}

async fn usage(pool: &PgPool, owner: Uuid) -> (i64, i64) {
    sqlx::query_as("SELECT u.operation_bytes, COALESCE((SELECT sum(octet_length(o.nonce)+octet_length(o.ciphertext)+octet_length(o.signature)+192)::bigint FROM operation_log o JOIN security_spaces s ON s.id=o.space_id WHERE s.owner_user_id=u.id),0) FROM users u WHERE id=$1")
        .bind(owner).fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn deleting_operations_spaces_and_workspaces_releases_only_their_charge() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let owner = Uuid::new_v4();
    let (workspace, first) = operation_fixture(&pool, owner).await;
    let other_space = Uuid::new_v4();
    sqlx::query("INSERT INTO security_spaces (id, workspace_id, owner_user_id, created_by, encrypted_metadata) VALUES ($1,$2,$3,$3,$4)")
        .bind(other_space).bind(workspace).bind(owner).bind(b"metadata".as_slice()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO security_space_members (id, space_id, user_id, role, key_epoch) VALUES ($1,$2,$3,'owner',1)")
        .bind(Uuid::new_v4()).bind(other_space).bind(owner).execute(&pool).await.unwrap();
    for (space, size) in [(first.space_id, 512_i64), (other_space, 1024)] {
        let blob_id = Uuid::new_v4();
        sqlx::query("INSERT INTO space_blobs (id, space_id, owner_user_id, created_by, ciphertext_sha256, size_padded, object_key, status) VALUES ($1,$2,$3,$3,$4,$5,$6,'ready')")
            .bind(blob_id).bind(space).bind(owner).bind(vec![31_u8;32]).bind(size).bind(format!("test/{space}/{blob_id}"))
            .execute(&pool).await.unwrap();
    }
    let mut second = first.clone();
    second.space_id = other_space;
    second.client_op_id = Uuid::new_v4();
    second.sign(&SigningKey::from_bytes(&[17; 32]));
    assert!(matches!(
        append_operation(&pool, owner, &first, 1_000_000, 1_000_000)
            .await
            .unwrap(),
        AppendResult::Accepted(1)
    ));
    assert!(matches!(
        append_operation(&pool, owner, &second, 1_000_000, 1_000_000)
            .await
            .unwrap(),
        AppendResult::Accepted(1)
    ));
    let charge =
        i64::try_from(first.nonce.len() + first.ciphertext.len() + first.signature.len() + 192)
            .unwrap();
    assert_eq!(usage(&pool, owner).await, (2 * charge, 2 * charge));
    sqlx::query("DELETE FROM operation_log WHERE space_id=$1")
        .bind(first.space_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(usage(&pool, owner).await, (charge, charge));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT blob_storage_bytes FROM users WHERE id=$1")
            .bind(owner)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1536
    );
    let mut replacement = first.clone();
    replacement.client_op_id = Uuid::new_v4();
    replacement.sign(&SigningKey::from_bytes(&[17; 32]));
    assert!(matches!(
        append_operation(&pool, owner, &replacement, 1_000_000, 1_000_000)
            .await
            .unwrap(),
        AppendResult::Accepted(2)
    ));
    sqlx::query("DELETE FROM security_spaces WHERE id=$1")
        .bind(first.space_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        usage(&pool, owner).await,
        (charge, charge),
        "space purge must preserve only other space usage"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT blob_storage_bytes FROM users WHERE id=$1")
            .bind(owner)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1024
    );
    sqlx::query("DELETE FROM workspaces WHERE id=$1")
        .bind(workspace)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(usage(&pool, owner).await, (0, 0));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT blob_storage_bytes FROM users WHERE id=$1")
            .bind(owner)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DELETE FROM users WHERE id=$1")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn distinct_concurrent_appends_allocate_sequences_without_lock_upgrade_deadlock() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let owner = Uuid::new_v4();
    let (_, first) = operation_fixture(&pool, owner).await;
    let mut second = first.clone();
    second.client_op_id = Uuid::new_v4();
    second.sign(&SigningKey::from_bytes(&[17; 32]));
    let label = format!("append-concurrency-{}", Uuid::new_v4());
    let url = std::env::var("KAMORI_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap();
    let options: sqlx::postgres::PgConnectOptions = url.parse().unwrap();
    let workers = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options.application_name(&label))
        .await
        .unwrap();
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM security_spaces WHERE id=$1 FOR SHARE")
        .bind(first.space_id)
        .execute(&mut *gate)
        .await
        .unwrap();
    let a_pool = workers.clone();
    let a = first.clone();
    let left =
        tokio::spawn(
            async move { append_operation(&a_pool, owner, &a, 1_000_000, 1_000_000).await },
        );
    let b_pool = workers.clone();
    let right = tokio::spawn(async move {
        append_operation(&b_pool, owner, &second, 1_000_000, 1_000_000).await
    });
    // Observe BOTH real requests blocked at the space boundary before opening
    // the gate. The old SHARE->UPDATE path now deterministically deadlocks;
    // the UPDATE-first path queues both and makes progress after release.
    let waiting=tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let count: i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE application_name=$1 AND wait_event_type='Lock'")
                .bind(&label).fetch_one(&pool).await.unwrap();
            if count==2 { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await;
    gate.commit().await.unwrap();
    waiting.expect("both appends must reach the locked space boundary");
    let (left, right) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(left, right)
    })
    .await
    .unwrap();
    let mut sequences = [
        left.unwrap()
            .expect("left distinct append must not deadlock"),
        right
            .unwrap()
            .expect("right distinct append must not deadlock"),
    ]
    .map(|result| match result {
        AppendResult::Accepted(sequence) => sequence,
        _ => panic!("distinct operation must be accepted"),
    });
    sequences.sort();
    assert_eq!(sequences, [1, 2]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT next_sequence FROM security_spaces WHERE id=$1")
            .bind(first.space_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    let charge =
        i64::try_from(first.nonce.len() + first.ciphertext.len() + first.signature.len() + 192)
            .unwrap();
    assert_eq!(usage(&pool, owner).await, (2 * charge, 2 * charge));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT operation_bytes FROM security_spaces WHERE id=$1")
            .bind(first.space_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        2 * charge
    );
    workers.close().await;
    sqlx::query("DELETE FROM workspaces WHERE owner_user_id=$1")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id=$1")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn operation_usage_migration_repairs_live_and_empty_accounts_after_newer_migration() {
    let Some(pool) = test_pool().await else {
        return;
    };
    // Identifier contains only this literal prefix and locally generated hex.
    let schema = format!("usage_migration_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&pool)
        .await
        .unwrap();
    let url = std::env::var("KAMORI_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap();
    let options: sqlx::postgres::PgConnectOptions = url.parse().unwrap();
    let isolated = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options.options([("search_path", schema.as_str())]))
        .await
        .unwrap();
    let before = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|m| m.version != 202610150002)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    // Reproduce the controller's real upgrade order: 003 already applied, 002
    // introduced later. Exercise SQLx's migration ledger, not just raw SQL.
    before.run(&isolated).await.unwrap();
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=202610150003)"
        )
        .fetch_one(&isolated)
        .await
        .unwrap()
    );
    let live = Uuid::new_v4();
    let empty = Uuid::new_v4();
    let (_, live_envelope) = operation_fixture(&isolated, live).await;
    let (empty_workspace, empty_envelope) = operation_fixture(&isolated, empty).await;
    for (owner, envelope) in [(live, &live_envelope), (empty, &empty_envelope)] {
        assert!(matches!(
            append_operation(&isolated, owner, envelope, 1_000_000, 1_000_000)
                .await
                .unwrap(),
            AppendResult::Accepted(1)
        ));
    }
    sqlx::query("DELETE FROM workspaces WHERE id=$1")
        .bind(empty_workspace)
        .execute(&isolated)
        .await
        .unwrap();
    let charge = i64::try_from(
        live_envelope.nonce.len()
            + live_envelope.ciphertext.len()
            + live_envelope.signature.len()
            + 192,
    )
    .unwrap();
    assert_eq!(
        usage(&isolated, empty).await,
        (charge, 0),
        "old trigger must reproduce existing drift"
    );
    sqlx::query("UPDATE users SET operation_bytes=operation_bytes+999 WHERE id=$1")
        .bind(live)
        .execute(&isolated)
        .await
        .unwrap();
    sqlx::query("UPDATE security_spaces SET operation_bytes=operation_bytes+123 WHERE id=$1")
        .bind(live_envelope.space_id)
        .execute(&isolated)
        .await
        .unwrap();
    MIGRATOR.run(&isolated).await.unwrap();
    assert_eq!(usage(&isolated, live).await, (charge, charge));
    assert_eq!(usage(&isolated, empty).await, (0, 0));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT operation_bytes FROM security_spaces WHERE id=$1")
            .bind(live_envelope.space_id)
            .fetch_one(&isolated)
            .await
            .unwrap(),
        charge
    );
    // Also exercise a fresh install in an empty schema through SQLx.
    isolated.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&pool)
        .await
        .unwrap();
    let options: sqlx::postgres::PgConnectOptions = url.parse().unwrap();
    let fresh = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.options([("search_path", schema.as_str())]))
        .await
        .unwrap();
    MIGRATOR.run(&fresh).await.unwrap();
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=202610150002)"
        )
        .fetch_one(&fresh)
        .await
        .unwrap()
    );
    fresh.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn duplicate_append_still_rejects_revoked_device_member_and_stale_epoch() {
    let Some(pool) = test_pool().await else {
        return;
    };
    for changed in ["device", "member", "epoch"] {
        let owner = Uuid::new_v4();
        let (workspace, envelope) = operation_fixture(&pool, owner).await;
        assert!(matches!(
            append_operation(&pool, owner, &envelope, 1_000_000, 1_000_000)
                .await
                .unwrap(),
            AppendResult::Accepted(1)
        ));
        match changed {
            "device" => {
                sqlx::query("UPDATE devices SET status='revoked', revoked_at=now() WHERE id=$1")
                    .bind(envelope.author_device_id)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            "member" => {
                sqlx::query("UPDATE security_space_members SET status='revoked', revoked_at=now() WHERE space_id=$1 AND user_id=$2").bind(envelope.space_id).bind(owner).execute(&pool).await.unwrap();
            }
            _ => {
                sqlx::query("UPDATE security_spaces SET current_key_epoch=2 WHERE id=$1")
                    .bind(envelope.space_id)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
        }
        assert!(matches!(
            append_operation(&pool, owner, &envelope, 1_000_000, 1_000_000)
                .await
                .unwrap(),
            AppendResult::AccessDeniedOrStaleEpoch
        ));
        sqlx::query("DELETE FROM workspaces WHERE id=$1")
            .bind(workspace)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id=$1")
            .bind(owner)
            .execute(&pool)
            .await
            .unwrap();
    }
}
