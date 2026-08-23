//! PostgreSQL-backed invite authorization and redemption invariants.

use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use super::repositories::{
    InviteCodeInsert, InviteCodeInsertResult, RedeemInviteOutcome, insert_invite_code,
    redeem_invite_code_tx,
};
use crate::features::spaces::{dto::SpaceRole, repositories::list_spaces};

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

async fn insert_user(pool: &PgPool, user_id: Uuid, label: &str) {
    sqlx::query(
        r#"
        INSERT INTO users (
            id, username, opaque_record, encrypted_master_key,
            public_key_bundle, recovery_verifier_hash
        ) VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(user_id)
    .bind(format!("invite-{label}-{user_id}"))
    .bind(vec![1_u8])
    .bind(vec![2_u8; 49])
    .bind(vec![3_u8])
    .bind(vec![4_u8; 32])
    .execute(pool)
    .await
    .expect("insert user");
}

fn invite<'a>(
    id: Uuid,
    space_id: Uuid,
    rotation_id: Uuid,
    created_by: Uuid,
    role: SpaceRole,
    code_hash: &'a [u8],
) -> InviteCodeInsert<'a> {
    let request_hash: &[u8; 32] = code_hash.try_into().expect("32-byte request hash");
    InviteCodeInsert {
        id,
        space_id,
        rotation_id,
        created_by,
        role,
        code_hash,
        encrypted_key_package: b"encrypted-space-key",
        encrypted_note: None,
        ttl_minutes: 60,
        request_hash,
    }
}

fn unique_code_hash() -> [u8; 32] {
    let mut hash = [0_u8; 32];
    hash[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    hash[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    hash
}

#[tokio::test]
async fn invite_creation_and_redemption_preserve_authorization_invariants() {
    let Some(pool) = test_pool().await else {
        eprintln!("skipping invite integration test: DATABASE_URL is not set");
        return;
    };
    let owner_id = Uuid::new_v4();
    let editor_id = Uuid::new_v4();
    let reader_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    for (user_id, label) in [
        (owner_id, "owner"),
        (editor_id, "editor"),
        (reader_id, "reader"),
        (recipient_id, "recipient"),
    ] {
        insert_user(&pool, user_id, label).await;
    }
    let workspace_id = Uuid::new_v4();
    let space_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, owner_user_id, kind, encrypted_metadata) VALUES ($1, $2, 'personal', $3)",
    )
    .bind(workspace_id)
    .bind(owner_id)
    .bind(vec![5_u8])
    .execute(&pool)
    .await
    .expect("insert workspace");
    sqlx::query(
        "INSERT INTO security_spaces (id, workspace_id, owner_user_id, created_by, encrypted_metadata) VALUES ($1, $2, $3, $3, $4)",
    )
    .bind(space_id)
    .bind(workspace_id)
    .bind(owner_id)
    .bind(vec![6_u8])
    .execute(&pool)
    .await
    .expect("insert space");
    sqlx::query(
        "UPDATE security_spaces SET current_key_epoch = 2, next_sequence = 3, current_state_start_seq = 2 WHERE id = $1",
    )
    .bind(space_id)
    .execute(&pool)
    .await
    .expect("advance test space epoch");
    let rotation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO security_space_epochs (space_id, key_epoch, rotation_id, status, created_by, committed_at) VALUES ($1, 2, $2, 'committed', $3, now())",
    )
    .bind(space_id)
    .bind(rotation_id)
    .bind(owner_id)
    .execute(&pool)
    .await
    .expect("insert prepared epoch");
    for (user_id, role) in [
        (owner_id, "owner"),
        (editor_id, "editor"),
        (reader_id, "reader"),
    ] {
        sqlx::query(
            "INSERT INTO security_space_members (id, space_id, user_id, role, key_epoch) VALUES ($1, $2, $3, $4, 2)",
        )
        .bind(Uuid::new_v4())
        .bind(space_id)
        .bind(user_id)
        .bind(role)
        .execute(&pool)
        .await
        .expect("insert membership");
    }
    let author_device_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO devices (id, user_id, signing_public_key, hpke_public_key, encrypted_name, platform) VALUES ($1, $2, $3, $4, $5, 'web')",
    )
    .bind(author_device_id)
    .bind(owner_id)
    .bind(vec![7_u8; 32])
    .bind(vec![8_u8; 32])
    .bind(vec![9_u8])
    .execute(&pool)
    .await
    .expect("insert operation author device");
    sqlx::query(
        "INSERT INTO operation_log (space_id, space_seq, stream_id, client_op_id, author_device_id, key_epoch, envelope_kind, cipher_suite, nonce, ciphertext, signature) VALUES ($1, 3, $2, $3, $4, 2, 'snapshot', 'xchacha20_poly1305', $5, $6, $7)",
    )
    .bind(space_id)
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(author_device_id)
    .bind(vec![10_u8; 24])
    .bind(vec![11_u8])
    .bind(vec![12_u8; 64])
    .execute(&pool)
    .await
    .expect("insert current-state snapshot");

    let denied_hash = unique_code_hash();
    assert!(matches!(
        insert_invite_code(
            &pool,
            invite(
                Uuid::new_v4(),
                space_id,
                rotation_id,
                reader_id,
                SpaceRole::Reader,
                &denied_hash,
            ),
        )
        .await
        .expect("reader invite admission"),
        InviteCodeInsertResult::AccessDenied
    ));

    let owner_hash = unique_code_hash();
    let owner_invite_id = Uuid::new_v4();
    assert!(matches!(
        insert_invite_code(
            &pool,
            invite(
                owner_invite_id,
                space_id,
                rotation_id,
                owner_id,
                SpaceRole::Reader,
                &owner_hash,
            ),
        )
        .await
        .expect("owner creates invite"),
        InviteCodeInsertResult::Stored(id) if id == owner_invite_id
    ));
    assert!(matches!(
        insert_invite_code(
            &pool,
            invite(
                Uuid::new_v4(),
                space_id,
                rotation_id,
                owner_id,
                SpaceRole::Reader,
                &owner_hash,
            ),
        )
        .await
        .expect("idempotent owner invite retry"),
        InviteCodeInsertResult::Stored(id) if id == owner_invite_id
    ));
    let conflicting_hash = unique_code_hash();
    assert!(matches!(
        insert_invite_code(
            &pool,
            invite(
                Uuid::new_v4(),
                space_id,
                rotation_id,
                owner_id,
                SpaceRole::Reader,
                &conflicting_hash,
            ),
        )
        .await
        .expect("conflicting owner invite retry"),
        InviteCodeInsertResult::Conflict
    ));
    assert!(matches!(
        redeem_invite_code_tx(&pool, &owner_hash, owner_id)
            .await
            .expect("owner redemption result"),
        RedeemInviteOutcome::AlreadyOwner
    ));
    let owner_invite_uses: i32 =
        sqlx::query_scalar("SELECT used_count FROM security_space_invites WHERE id = $1")
            .bind(owner_invite_id)
            .fetch_one(&pool)
            .await
            .expect("load owner invite usage");
    assert_eq!(owner_invite_uses, 0);

    let redeemed = redeem_invite_code_tx(&pool, &owner_hash, recipient_id)
        .await
        .expect("recipient redemption");
    let RedeemInviteOutcome::Redeemed(redeemed) = redeemed else {
        panic!("recipient should redeem invite");
    };
    assert_eq!(redeemed.role, SpaceRole::Reader);
    assert_eq!(redeemed.history_start_seq, 2);
    assert_eq!(redeemed.current_state_start_seq, 2);
    let spaces = list_spaces(&pool, recipient_id)
        .await
        .expect("list recipient spaces");
    let summary = spaces
        .iter()
        .find(|space| space.space_id == space_id)
        .expect("redeemed space summary");
    assert_eq!(summary.history_start_seq, 2);
    assert_eq!(summary.current_state_start_seq, 2);

    pool.close().await;
}
