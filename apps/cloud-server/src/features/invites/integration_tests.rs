//! PostgreSQL-backed invite authorization and redemption invariants.

use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use super::repositories::{
    InviteCodeInsert, InviteCodeInsertResult, RedeemInviteOutcome, insert_invite_code,
    redeem_invite_code_tx,
};
use crate::features::spaces::{dto::SpaceRole, repositories::list_spaces};

use crate::platform::test_support::required_test_database_url;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

async fn test_pool() -> PgPool {
    let database_url = required_test_database_url();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect postgres");
    MIGRATOR.run(&pool).await.expect("run migrations");
    pool
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
    let pool = test_pool().await;
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

async fn redemption_fixture(
    pool: &PgPool,
    recipient: Uuid,
    existing_role: Option<&str>,
) -> (Uuid, Uuid, [u8; 32]) {
    let owner = Uuid::new_v4();
    insert_user(pool, owner, "retry-owner").await;
    insert_user(pool, recipient, "retry-recipient").await;
    let workspace = Uuid::new_v4();
    let space = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, owner_user_id, kind, encrypted_metadata) VALUES ($1, $2, 'personal', $3)")
        .bind(workspace).bind(owner).bind(b"metadata".as_slice()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO security_spaces (id, workspace_id, owner_user_id, created_by, encrypted_metadata, current_key_epoch, next_sequence, current_state_start_seq) VALUES ($1, $2, $3, $3, $4, 2, 7, 5)")
        .bind(space).bind(workspace).bind(owner).bind(b"metadata".as_slice()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO security_space_members (id, space_id, user_id, role, key_epoch) VALUES ($1, $2, $3, 'owner', 2)")
        .bind(Uuid::new_v4()).bind(space).bind(owner).execute(pool).await.unwrap();
    if let Some(role) = existing_role {
        sqlx::query("INSERT INTO security_space_members (id, space_id, user_id, role, key_epoch, history_start_seq) VALUES ($1, $2, $3, $4, 2, 1)")
            .bind(Uuid::new_v4()).bind(space).bind(recipient).bind(role).execute(pool).await.unwrap();
    }
    let rotation = Uuid::new_v4();
    sqlx::query("INSERT INTO security_space_epochs (space_id, key_epoch, rotation_id, status, created_by, committed_at) VALUES ($1, 2, $2, 'committed', $3, now())")
        .bind(space).bind(rotation).bind(owner).execute(pool).await.unwrap();
    let hash = unique_code_hash();
    let mut input = invite(
        Uuid::new_v4(),
        space,
        rotation,
        owner,
        SpaceRole::Reader,
        &hash,
    );
    input.encrypted_note = Some(b"retry note");
    assert!(matches!(
        insert_invite_code(pool, input).await.unwrap(),
        InviteCodeInsertResult::Stored(_)
    ));
    (owner, space, hash)
}

fn redeemed(outcome: RedeemInviteOutcome) -> super::repositories::RedeemedInvite {
    match outcome {
        RedeemInviteOutcome::Redeemed(value) => value,
        _ => panic!("expected successful exact invite redemption"),
    }
}

fn assert_same_redemption(
    left: &super::repositories::RedeemedInvite,
    right: &super::repositories::RedeemedInvite,
) {
    assert_eq!(left.space_id, right.space_id);
    assert_eq!(left.role, right.role);
    assert_eq!(left.key_epoch, right.key_epoch);
    assert_eq!(left.history_start_seq, right.history_start_seq);
    assert_eq!(left.current_state_start_seq, right.current_state_start_seq);
    assert_eq!(left.encrypted_key_package, right.encrypted_key_package);
    assert_eq!(left.encrypted_note, right.encrypted_note);
}

#[tokio::test]
async fn lost_redemption_response_retries_preserve_original_response_and_entitlement() {
    let pool = test_pool().await;
    for existing_role in [None, Some("editor")] {
        let actor = Uuid::new_v4();
        let (owner, space, hash) = redemption_fixture(&pool, actor, existing_role).await;
        let first = redeemed(redeem_invite_code_tx(&pool, &hash, actor).await.unwrap());
        assert_eq!(
            first.role,
            if existing_role.is_some() {
                SpaceRole::Editor
            } else {
                SpaceRole::Reader
            }
        );
        assert_eq!(
            first.history_start_seq,
            if existing_role.is_some() { 1 } else { 5 }
        );
        let before: (Uuid, String, i32, i64) = sqlx::query_as("SELECT id, role, key_epoch, history_start_seq FROM security_space_members WHERE space_id=$1 AND user_id=$2")
            .bind(space).bind(actor).fetch_one(&pool).await.unwrap();
        // Completion is durable even after the unused-code admission deadline.
        sqlx::query("UPDATE security_space_invites SET expires_at=now()-interval '1 second' WHERE code_hash=$1")
            .bind(hash.as_slice()).execute(&pool).await.unwrap();
        // The original response floor is durable, even if server metadata later
        // advances within this epoch. A retry does not rewrite the entitlement.
        sqlx::query("UPDATE security_spaces SET current_state_start_seq=6 WHERE id=$1")
            .bind(space)
            .execute(&pool)
            .await
            .unwrap();
        let retry = redeemed(redeem_invite_code_tx(&pool, &hash, actor).await.unwrap());
        assert_same_redemption(&first, &retry);
        let after: (Uuid, String, i32, i64) = sqlx::query_as("SELECT id, role, key_epoch, history_start_seq FROM security_space_members WHERE space_id=$1 AND user_id=$2")
            .bind(space).bind(actor).fetch_one(&pool).await.unwrap();
        assert_eq!(before, after);
        assert_eq!(
            sqlx::query_scalar::<_, i32>(
                "SELECT used_count FROM security_space_invites WHERE code_hash=$1"
            )
            .bind(hash.as_slice())
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        sqlx::query("DELETE FROM workspaces WHERE owner_user_id=$1")
            .bind(owner)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id IN ($1,$2)")
            .bind(owner)
            .bind(actor)
            .execute(&pool)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn concurrent_same_recipient_redemptions_return_identical_response() {
    let pool = test_pool().await;
    let actor = Uuid::new_v4();
    let (owner, space, hash) = redemption_fixture(&pool, actor, None).await;
    let (left, right) = tokio::join!(
        redeem_invite_code_tx(&pool, &hash, actor),
        redeem_invite_code_tx(&pool, &hash, actor)
    );
    assert_same_redemption(&redeemed(left.unwrap()), &redeemed(right.unwrap()));
    assert_eq!(
        sqlx::query_scalar::<_, i32>(
            "SELECT used_count FROM security_space_invites WHERE code_hash=$1"
        )
        .bind(hash.as_slice())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM security_space_members WHERE space_id=$1 AND user_id=$2"
        )
        .bind(space)
        .bind(actor)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    sqlx::query("DELETE FROM workspaces WHERE owner_user_id=$1")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id IN ($1,$2)")
        .bind(owner)
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn concurrent_different_recipients_cannot_share_single_use_invite() {
    let pool = test_pool().await;
    let actor = Uuid::new_v4();
    let other = Uuid::new_v4();
    let (owner, space, hash) = redemption_fixture(&pool, actor, None).await;
    insert_user(&pool, other, "other").await;
    let (left, right) = tokio::join!(
        redeem_invite_code_tx(&pool, &hash, actor),
        redeem_invite_code_tx(&pool, &hash, other)
    );
    let outcomes = [left.unwrap(), right.unwrap()];
    assert_eq!(
        outcomes
            .iter()
            .filter(|v| matches!(v, RedeemInviteOutcome::Redeemed(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|v| matches!(v, RedeemInviteOutcome::InvalidOrExpired))
            .count(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i32>(
            "SELECT used_count FROM security_space_invites WHERE code_hash=$1"
        )
        .bind(hash.as_slice())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM security_space_members WHERE space_id=$1 AND user_id IN ($2,$3)"
        )
        .bind(space)
        .bind(actor)
        .bind(other)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    sqlx::query("DELETE FROM workspaces WHERE owner_user_id=$1")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id IN ($1,$2,$3)")
        .bind(owner)
        .bind(actor)
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn retries_reject_removed_members_stale_epochs_and_inactive_spaces() {
    let pool = test_pool().await;
    for mutation in ["removed", "epoch", "space", "revoked"] {
        let actor = Uuid::new_v4();
        let (owner, space, hash) = redemption_fixture(&pool, actor, None).await;
        redeemed(redeem_invite_code_tx(&pool, &hash, actor).await.unwrap());
        match mutation {
            "removed" => {
                sqlx::query("UPDATE security_space_members SET status='revoked', revoked_at=now() WHERE space_id=$1 AND user_id=$2").bind(space).bind(actor).execute(&pool).await.unwrap();
            }
            "epoch" => {
                sqlx::query("UPDATE security_spaces SET current_key_epoch=3 WHERE id=$1")
                    .bind(space)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            "space" => {
                sqlx::query(
                    "UPDATE security_spaces SET status='deleted', deleted_at=now() WHERE id=$1",
                )
                .bind(space)
                .execute(&pool)
                .await
                .unwrap();
            }
            _ => {
                sqlx::query(
                    "UPDATE security_space_invites SET revoked_at=now() WHERE code_hash=$1",
                )
                .bind(hash.as_slice())
                .execute(&pool)
                .await
                .unwrap();
            }
        }
        assert!(matches!(
            redeem_invite_code_tx(&pool, &hash, actor).await.unwrap(),
            RedeemInviteOutcome::InvalidOrExpired
        ));
        assert_eq!(
            sqlx::query_scalar::<_, i32>(
                "SELECT used_count FROM security_space_invites WHERE code_hash=$1"
            )
            .bind(hash.as_slice())
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        sqlx::query("DELETE FROM workspaces WHERE owner_user_id=$1")
            .bind(owner)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id IN ($1,$2)")
            .bind(owner)
            .bind(actor)
            .execute(&pool)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn expired_unused_invite_never_activates_membership() {
    let pool = test_pool().await;
    let actor = Uuid::new_v4();
    let (owner, space, hash) = redemption_fixture(&pool, actor, None).await;
    sqlx::query(
        "UPDATE security_space_invites SET expires_at=now()-interval '1 second' WHERE code_hash=$1",
    )
    .bind(hash.as_slice())
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        redeem_invite_code_tx(&pool, &hash, actor).await.unwrap(),
        RedeemInviteOutcome::InvalidOrExpired
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM security_space_members WHERE space_id=$1 AND user_id=$2"
        )
        .bind(space)
        .bind(actor)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i32>(
            "SELECT used_count FROM security_space_invites WHERE code_hash=$1"
        )
        .bind(hash.as_slice())
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    sqlx::query("DELETE FROM workspaces WHERE owner_user_id=$1")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id IN ($1,$2)")
        .bind(owner)
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn redemption_retry_preserves_entitlement_after_recipient_becomes_owner() {
    use crate::features::ownership::{
        dto::OwnershipResourceKind,
        repositories::{AcceptOfferResult, CreateOfferResult, accept_offer, create_offer},
    };
    let pool = test_pool().await;
    let actor = Uuid::new_v4();
    let (owner, space, hash) = redemption_fixture(&pool, actor, None).await;
    let original = redeemed(redeem_invite_code_tx(&pool, &hash, actor).await.unwrap());
    let CreateOfferResult::Created(offer) = create_offer(
        &pool,
        owner,
        OwnershipResourceKind::SecuritySpace,
        space,
        actor,
    )
    .await
    .unwrap() else {
        panic!("offer must be created");
    };
    assert!(matches!(
        accept_offer(&pool, actor, offer.transfer_id, i64::MAX, i64::MAX)
            .await
            .unwrap(),
        AcceptOfferResult::Accepted
    ));
    let retry = redeemed(redeem_invite_code_tx(&pool, &hash, actor).await.unwrap());
    assert_same_redemption(&original, &retry);
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT role FROM security_space_members WHERE space_id=$1 AND user_id=$2"
        )
        .bind(space)
        .bind(actor)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "owner"
    );
    sqlx::query("DELETE FROM workspaces WHERE owner_user_id=$1")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id IN ($1,$2)")
        .bind(owner)
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
}
