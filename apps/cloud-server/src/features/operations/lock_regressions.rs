// Exercise complete production transactions, staged by observed PostgreSQL waits.
use crate::features::{
    auth::repositories::{
        AccountRecoveryReset, AuthenticationBinding, apply_account_recovery_reset,
        create_account_recovery_attempt,
    },
    cas::repositories::{StoreBlobResult, store_blob},
    spaces::{
        dto::{DeviceKeyPackage, MemberRecoveryKeyPackage},
        repositories::{MemberRotation, RevokeMemberResult, revoke_member_and_rotate},
    },
};
use std::time::Duration;

async fn named_pool(label: &str) -> PgPool {
    let url = required_test_database_url();
    let options: sqlx::postgres::PgConnectOptions = url.parse().unwrap();
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.application_name(label))
        .await
        .unwrap()
}
async fn waiting(pool: &PgPool, label: &str) {
    tokio::time::timeout(Duration::from_secs(10),async {
        loop {
            let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE application_name=$1 AND cardinality(pg_blocking_pids(pid))>0 AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_stat_activity.pid AND NOT l.granted AND l.locktype IN ('transactionid','advisory','tuple')))").bind(label).fetch_one(pool).await.unwrap();
            if blocked {break;}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap_or_else(|_| panic!("production transaction {label} must reach observed lock wait"));
}
async fn clean(pool: &PgPool, owner: Uuid) {
    sqlx::query("DELETE FROM workspaces WHERE owner_user_id=$1")
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id=$1")
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn final_rotation_and_recovery_serialize_in_both_orders() {
    let pool = test_pool().await;
    for recovery_first in [true, false] {
        let owner = Uuid::new_v4();
        let (_, envelope) = operation_fixture(&pool, owner).await;
        append_operation(&pool, owner, &envelope, 1_000_000, 1_000_000)
            .await
            .unwrap();
        let token = *Uuid::new_v4().as_bytes();
        let mut token_hash = [0; 32];
        token_hash[..16].copy_from_slice(&token);
        create_account_recovery_attempt(
            &pool,
            &token_hash,
            owner,
            0,
            time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        )
        .await
        .unwrap();
        sqlx::query("INSERT INTO refresh_tokens(id,user_id,token_hash,expires_at) VALUES($1,$2,$3,now()+interval '1 day')")
            .bind(Uuid::new_v4()).bind(owner).bind(token_hash.as_slice()).execute(&pool).await.unwrap();
        let space_id = envelope.space_id;
        let rotate_label = format!("rotate-{owner}");
        let recover_label = format!("recover-{owner}");
        let rotate_pool = named_pool(&rotate_label).await;
        let recover_pool = named_pool(&recover_label).await;
        let mut gate = pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM users WHERE id=$1 FOR NO KEY UPDATE")
            .bind(owner)
            .execute(&mut *gate)
            .await
            .unwrap();
        let rotate = async move {
            let mut snapshot = envelope.clone();
            snapshot.key_epoch = 2;
            snapshot.envelope_kind = EnvelopeKind::Snapshot;
            snapshot.client_op_id = Uuid::new_v4();
            snapshot.sign(&SigningKey::from_bytes(&[17; 32]));
            revoke_member_and_rotate(
                &rotate_pool,
                MemberRotation {
                    actor_id: owner,
                    space_id: envelope.space_id,
                    target_user_id: None,
                    expected_key_epoch: 1,
                    new_key_epoch: 2,
                    base_space_seq: 1,
                    rotation_id: Uuid::new_v4(),
                    new_encrypted_metadata: b"metadata",
                    packages: &[DeviceKeyPackage {
                        device_id: envelope.author_device_id,
                        key_epoch: 2,
                        encrypted_key_package: vec![1],
                    }],
                    recovery_packages: &[MemberRecoveryKeyPackage {
                        user_id: owner,
                        key_epoch: 2,
                        encrypted_key_package: vec![2; 49],
                    }],
                    snapshots: &[snapshot],
                    quarantined_streams: &[],
                    request_hash: &[8; 32],
                },
            )
            .await
        };
        let recover = async move {
            apply_account_recovery_reset(
                &recover_pool,
                AccountRecoveryReset {
                    user_id: owner,
                    token_hash: &token_hash,
                    request_hash: &[7; 32],
                    opaque_record: &[3],
                    encrypted_master_key: &[4; 49],
                },
            )
            .await
        };
        let (rotation, recovery) = if recovery_first {
            let recovery = tokio::spawn(recover);
            waiting(&pool, &recover_label).await;
            let rotation = tokio::spawn(rotate);
            waiting(&pool, &rotate_label).await;
            (rotation, recovery)
        } else {
            let rotation = tokio::spawn(rotate);
            waiting(&pool, &rotate_label).await;
            let recovery = tokio::spawn(recover);
            waiting(&pool, &recover_label).await;
            (rotation, recovery)
        };
        gate.commit().await.unwrap();
        let (rotation, recovery) = tokio::time::timeout(Duration::from_secs(15), async {
            tokio::join!(rotation, recovery)
        })
        .await
        .unwrap();
        let rotation = rotation.unwrap();
        let recovery = recovery.unwrap();
        let totals = usage(&pool, owner).await;
        let generation: i64 = sqlx::query_scalar("SELECT auth_generation FROM users WHERE id=$1")
            .bind(owner)
            .fetch_one(&pool)
            .await
            .unwrap();
        let active: i64 =
            sqlx::query_scalar("SELECT count(*) FROM devices WHERE user_id=$1 AND status='active'")
                .bind(owner)
                .fetch_one(&pool)
                .await
                .unwrap();
        let epoch: i32 =
            sqlx::query_scalar("SELECT current_key_epoch FROM security_spaces WHERE id=$1")
                .bind(space_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let sessions: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM refresh_tokens WHERE user_id=$1 AND revoked_at IS NULL",
        )
        .bind(owner)
        .fetch_one(&pool)
        .await
        .unwrap();
        clean(&pool, owner).await;
        recovery.expect("full recovery must not deadlock");
        let rotation = rotation.expect("snapshot rotation must not deadlock");
        if recovery_first {
            assert!(matches!(
                rotation,
                RevokeMemberResult::PackageCoverageMismatch
            ));
            assert_eq!(epoch, 1);
            assert_eq!(totals, (282, 282));
        } else {
            assert!(matches!(rotation, RevokeMemberResult::Revoked));
            assert_eq!(epoch, 2);
            assert_eq!(totals, (564, 564));
        }
        assert_eq!(sessions, 0);
        assert_eq!(generation, 1);
        assert_eq!(active, 0);
    }
}

#[tokio::test]
async fn final_blob_reservation_and_ownership_accept_serialize() {
    let pool = test_pool().await;
    let old = Uuid::new_v4();
    let recipient = Uuid::new_v4();
    let (_, from) = operation_fixture(&pool, old).await;
    let (_, to) = operation_fixture(&pool, recipient).await;
    append_operation(&pool, old, &from, 1_000_000, 1_000_000)
        .await
        .unwrap();
    store_blob(
        &pool,
        old,
        from.space_id,
        Uuid::new_v4(),
        &[3; 32],
        512,
        1_000_000,
    )
    .await
    .unwrap();
    sqlx::query("INSERT INTO security_space_members(id,space_id,user_id,role,key_epoch) VALUES($1,$2,$3,'editor',1)").bind(Uuid::new_v4()).bind(from.space_id).bind(recipient).execute(&pool).await.unwrap();
    let offer = match create_offer(
        &pool,
        old,
        OwnershipResourceKind::SecuritySpace,
        from.space_id,
        recipient,
    )
    .await
    .unwrap()
    {
        CreateOfferResult::Created(offer) => offer,
        _ => panic!("offer"),
    };
    let blob_label = format!("blob-{old}");
    let accept_label = format!("accept-{old}");
    let blob_pool = named_pool(&blob_label).await;
    let accept_pool = named_pool(&accept_label).await;
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,731))")
        .bind(recipient.to_string())
        .execute(&mut *gate)
        .await
        .unwrap();
    let blob = tokio::spawn(async move {
        store_blob(
            &blob_pool,
            recipient,
            to.space_id,
            Uuid::new_v4(),
            &[5; 32],
            1024,
            1_000_000,
        )
        .await
    });
    waiting(&pool, &blob_label).await;
    let accept = tokio::spawn(async move {
        accept_offer(
            &accept_pool,
            recipient,
            offer.transfer_id,
            1_000_000,
            1_000_000,
        )
        .await
    });
    waiting(&pool, &accept_label).await;
    gate.commit().await.unwrap();
    let (blob, accept) = tokio::time::timeout(Duration::from_secs(15), async {
        tokio::join!(blob, accept)
    })
    .await
    .unwrap();
    let old_usage = usage(&pool, old).await;
    let new_usage = usage(&pool, recipient).await;
    let blob_usage: i64 = sqlx::query_scalar("SELECT blob_storage_bytes FROM users WHERE id=$1")
        .bind(recipient)
        .fetch_one(&pool)
        .await
        .unwrap();
    clean(&pool, old).await;
    clean(&pool, recipient).await;
    assert!(matches!(
        blob.unwrap().expect("blob must not deadlock"),
        StoreBlobResult::NeedsUpload(_)
    ));
    assert!(matches!(
        accept.unwrap().expect("accept must not deadlock"),
        AcceptOfferResult::Accepted
    ));
    assert_eq!(old_usage, (0, 0));
    assert_eq!(new_usage.0, new_usage.1);
    assert!(new_usage.0 > 0);
    assert_eq!(blob_usage, 1536);
}

#[tokio::test]
async fn final_multispace_account_delete_and_append_serialize() {
    let pool = test_pool().await;
    let owner = Uuid::new_v4();
    let (workspace, mut envelope) = operation_fixture(&pool, owner).await;
    let second = Uuid::new_v4();
    sqlx::query("INSERT INTO security_spaces(id,workspace_id,owner_user_id,created_by,encrypted_metadata) VALUES($1,$2,$3,$3,$4)").bind(second).bind(workspace).bind(owner).bind(vec![1_u8]).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO security_space_members(id,space_id,user_id,role,key_epoch) VALUES($1,$2,$3,'owner',1)").bind(Uuid::new_v4()).bind(second).bind(owner).execute(&pool).await.unwrap();
    append_operation(&pool, owner, &envelope, 1_000_000, 1_000_000)
        .await
        .unwrap();
    let first = envelope.space_id;
    envelope.space_id = second;
    envelope.client_op_id = Uuid::new_v4();
    envelope.sign(&SigningKey::from_bytes(&[17; 32]));
    let append_label = format!("append-delete-{owner}");
    let delete_label = format!("delete-{owner}");
    let append_pool = named_pool(&append_label).await;
    let delete_pool = named_pool(&delete_label).await;
    // Pause the actual first row deletion after its accounting BEFORE trigger.
    // This test-only trigger makes the old row-at-a-time order deterministic.
    let trigger = format!("zz_delete_gate_{}", owner.simple());
    let gate_key = i64::from_be_bytes(owner.as_bytes()[..8].try_into().unwrap());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.owner_user_id = '{}'::uuid THEN PERFORM pg_advisory_xact_lock(hashtextextended(OLD.id::text,919)); PERFORM pg_advisory_xact_lock({gate_key}); END IF; RETURN OLD; END; $$; CREATE TRIGGER {trigger} BEFORE DELETE ON security_spaces FOR EACH ROW EXECUTE FUNCTION {trigger}();",owner))).execute(&pool).await.unwrap();
    // Gate the original first space; the appender targets the other space.
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(gate_key)
        .execute(&mut *gate)
        .await
        .unwrap();
    let delete = tokio::spawn(async move {
        let mut tx = delete_pool.begin().await.unwrap();
        let result = crate::features::users::repositories::delete_user(
            &mut tx,
            AuthenticationBinding {
                user_id: owner,
                generation: 0,
                session_id: None,
            },
        )
        .await;
        if result.is_ok() {
            tx.commit().await.unwrap();
        } else {
            tx.rollback().await.unwrap();
        }
        result
    });
    waiting(&pool, &delete_label).await;
    let mut probe = pool.begin().await.unwrap();
    let first_not_deleting: bool =
        sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1,919))")
            .bind(first.to_string())
            .fetch_one(&mut *probe)
            .await
            .unwrap();
    probe.rollback().await.unwrap();
    envelope.space_id = if first_not_deleting { first } else { second };
    envelope.sign(&SigningKey::from_bytes(&[17; 32]));
    let append = tokio::spawn(async move {
        append_operation(&append_pool, owner, &envelope, 1_000_000, 1_000_000).await
    });
    waiting(&pool, &append_label).await;
    gate.commit().await.unwrap();
    let (append, delete) = tokio::time::timeout(Duration::from_secs(15), async {
        tokio::join!(append, delete)
    })
    .await
    .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER {trigger} ON security_spaces; DROP FUNCTION {trigger}();"
    )))
    .execute(&pool)
    .await
    .unwrap();
    let totals = usage(&pool, owner).await;
    let deleted: bool = sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM users WHERE id=$1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .unwrap();
    clean(&pool, owner).await;
    assert!(
        delete
            .unwrap()
            .expect("bulk account delete must not deadlock")
    );
    assert!(matches!(
        append.unwrap().expect("append must not deadlock"),
        AppendResult::Accepted(_) | AppendResult::AccessDeniedOrStaleEpoch
    ));
    assert!(deleted);
    assert_eq!(totals, (0, 0));
}

#[tokio::test]
async fn final_maintenance_mixed_owner_purge_preserves_concurrent_upload_lease() {
    let admin = test_pool().await;
    // Maintenance deliberately scans its entire database: isolate every row and
    // queued object in a synthetic schema, never the controller's shared fixtures.
    let schema = format!("maintenance_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let url = required_test_database_url();
    let options: sqlx::postgres::PgConnectOptions = url.parse().unwrap();
    let options = options.options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options.clone())
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let (workspace, first) = operation_fixture(&pool, a).await;
    let (_, second) = operation_fixture(&pool, b).await;
    for (owner, envelope) in [(a, &first), (b, &second)] {
        append_operation(&pool, owner, envelope, 1_000_000, 1_000_000)
            .await
            .unwrap();
        store_blob(
            &pool,
            owner,
            envelope.space_id,
            Uuid::new_v4(),
            &[4; 32],
            512,
            1_000_000,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE security_spaces SET status='deleted',deleted_at=now()-interval '31 days' WHERE id=$1").bind(envelope.space_id).execute(&pool).await.unwrap();
    }
    let live = Uuid::new_v4();
    let blob = Uuid::new_v4();
    sqlx::query("INSERT INTO security_spaces(id,workspace_id,owner_user_id,created_by,encrypted_metadata) VALUES($1,$2,$3,$3,$4)").bind(live).bind(workspace).bind(a).bind(vec![1_u8]).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO security_space_members(id,space_id,user_id,role,key_epoch) VALUES($1,$2,$3,'owner',1)").bind(Uuid::new_v4()).bind(live).bind(a).execute(&pool).await.unwrap();
    store_blob(&pool, a, live, blob, &[5; 32], 1024, 1_000_000)
        .await
        .unwrap();
    sqlx::query("UPDATE space_blobs SET created_at=now()-interval '25 hours',upload_lease_until=now()-interval '1 hour' WHERE id=$1").bind(blob).execute(&pool).await.unwrap();
    let retry_label = format!("purge-retry-{a}");
    let purge_label = format!("purge-{a}");
    let retry_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone().application_name(&retry_label))
        .await
        .unwrap();
    let purge_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.application_name(&purge_label))
        .await
        .unwrap();
    let mut state = crate::platform::test_support::test_state();
    state.pool = purge_pool.clone();
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,731))")
        .bind(a.to_string())
        .execute(&mut *gate)
        .await
        .unwrap();
    // Upload retains the active space SHARE lock while waiting for quota731.
    let retry = tokio::spawn(async move {
        store_blob(&retry_pool, a, live, blob, &[5; 32], 1024, 1_000_000).await
    });
    waiting(&admin, &retry_label).await;
    let purge =
        tokio::spawn(async move { crate::platform::maintenance::prepare_deletions(&state).await });
    waiting(&admin, &purge_label).await;
    gate.commit().await.unwrap();
    let (retry, purge) = tokio::time::timeout(Duration::from_secs(15), async {
        tokio::join!(retry, purge)
    })
    .await
    .unwrap();
    assert!(matches!(
        retry.unwrap().unwrap(),
        StoreBlobResult::NeedsUpload(_)
    ));
    purge
        .unwrap()
        .expect("mixed-owner cleanup and upload retry serialize");
    assert_eq!(usage(&pool, a).await, (0, 0));
    assert_eq!(usage(&pool, b).await, (0, 0));
    let counters: Vec<i64> = sqlx::query_scalar(
        "SELECT blob_storage_bytes FROM users WHERE id IN ($1,$2) ORDER BY blob_storage_bytes",
    )
    .bind(a)
    .bind(b)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(counters, vec![0, 1024]);
    let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM object_deletion_queue")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(queued, 2);
    let renewed: bool =
        sqlx::query_scalar("SELECT upload_lease_until>now() FROM space_blobs WHERE id=$1")
            .bind(blob)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(renewed);
    purge_pool.close().await;
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
}

// Trigger DDL must not contend with another concurrency test's deliberately
// paused transaction. Each admission scenario owns a migrated synthetic schema.
async fn admission_database() -> (PgPool, PgPool, String) {
    let admin = test_pool().await;
    let schema = format!("admission_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let pool = admission_pool(&schema, "admission-observer", 2).await;
    MIGRATOR.run(&pool).await.unwrap();
    (admin, pool, schema)
}
async fn admission_pool(schema: &str, label: &str, connections: u32) -> PgPool {
    let url = required_test_database_url();
    let options: sqlx::postgres::PgConnectOptions = url.parse().unwrap();
    PgPoolOptions::new()
        .max_connections(connections)
        .connect_with(
            options
                .options([("search_path", schema)])
                .application_name(label),
        )
        .await
        .unwrap()
}
async fn drop_admission_database(admin: &PgPool, pool: PgPool, schema: &str) {
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(admin)
        .await
        .unwrap();
}

async fn admit_workspace(
    pool: &PgPool,
    owner: Uuid,
    personal: bool,
) -> Result<Uuid, crate::features::common::ApiError> {
    use crate::features::workspaces::repositories::{
        create_team_workspace_for_owner, ensure_personal_workspace_for_user,
    };
    if personal {
        ensure_personal_workspace_for_user(pool, owner).await
    } else {
        create_team_workspace_for_owner(pool, owner, &[1]).await
    }
}

async fn delete_account(
    pool: &PgPool,
    owner: Uuid,
) -> Result<bool, crate::features::common::ApiError> {
    let mut tx = pool.begin().await.unwrap();
    let result = crate::features::users::repositories::delete_user(
        &mut tx,
        AuthenticationBinding {
            user_id: owner,
            generation: 0,
            session_id: None,
        },
    )
    .await;
    if result.is_ok() {
        tx.commit().await.unwrap();
    } else {
        tx.rollback().await.unwrap();
    }
    result
}

// Observe admission itself: a generic blocked connection could instead be
// waiting for a late FK/user lock, which would not protect candidate discovery.
async fn waits_for_admission(pool: &PgPool, label: &str, owner: Uuid) -> bool {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity a JOIN pg_locks l ON l.pid=a.pid WHERE a.application_name=$1 AND NOT l.granted AND l.locktype='advisory' AND l.classid=((hashtextextended($2,913)>>32)&4294967295)::oid AND l.objid=(hashtextextended($2,913)&4294967295)::oid)")
        .bind(label).bind(owner.to_string()).fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn final_late_team_workspace_cannot_escape_account_deletion() {
    late_workspace_admission(false).await;
}
#[tokio::test]
async fn final_late_personal_workspace_cannot_escape_account_deletion() {
    late_workspace_admission(true).await;
}
async fn late_workspace_admission(personal: bool) {
    let (admin, pool, schema) = admission_database().await;
    let owner = Uuid::new_v4();
    let (workspace, mut envelope) = operation_fixture(&pool, owner).await;
    // Leave no personal workspace, so ensure must exercise its insert path.
    sqlx::query("UPDATE workspaces SET kind='team' WHERE id=$1")
        .bind(workspace)
        .execute(&pool)
        .await
        .unwrap();
    let label = format!("late-create-{owner}");
    let delete_label = format!("late-delete-{owner}");
    let create_pool = admission_pool(&schema, &label, 1).await;
    let delete_pool = admission_pool(&schema, &delete_label, 1).await;
    let append_label = format!("late-append-{owner}");
    let append_pool = admission_pool(&schema, &append_label, 1).await;
    let trigger = format!("zz_admission_gate_{}", owner.simple());
    let gate_key = i64::from_be_bytes(owner.as_bytes()[..8].try_into().unwrap());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.owner_user_id = '{owner}'::uuid THEN PERFORM pg_advisory_xact_lock({gate_key}); END IF; RETURN OLD; END; $$; CREATE TRIGGER {trigger} BEFORE DELETE ON security_spaces FOR EACH ROW EXECUTE FUNCTION {trigger}();"))).execute(&pool).await.unwrap();
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(gate_key)
        .execute(&mut *gate)
        .await
        .unwrap();
    let deletion = tokio::spawn(async move { delete_account(&delete_pool, owner).await });
    waiting(&pool, &delete_label).await;
    let mut creation =
        tokio::spawn(async move { admit_workspace(&create_pool, owner, personal).await });
    let early = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tokio::select! {
                result = &mut creation => break Some(result.unwrap()),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    if waits_for_admission(&pool, &label, owner).await { break None; }
                }
            }
        }
    })
    .await
    .expect("creation must complete or reach the admission gate");
    let mut append = None;
    let blocked = early.is_none();
    // On the old implementation exercise the complete reachable cycle, using
    // real workspace/space creation and signed append, not hand-written locks.
    if let Some(Ok(late_workspace)) = &early {
        use crate::features::spaces::{
            dto::CreateSpaceRequest,
            repositories::{CreateSpaceResult, create_space},
        };
        envelope.space_id = Uuid::new_v4();
        envelope.client_op_id = Uuid::new_v4();
        envelope.sign(&SigningKey::from_bytes(&[17; 32]));
        let result = create_space(
            &pool,
            owner,
            *late_workspace,
            &CreateSpaceRequest {
                workspace_id: Some(*late_workspace),
                space_id: envelope.space_id,
                encrypted_metadata: vec![1],
                device_key_packages: vec![DeviceKeyPackage {
                    device_id: envelope.author_device_id,
                    key_epoch: 1,
                    encrypted_key_package: vec![1],
                }],
                encrypted_recovery_key_package: vec![2; 49],
            },
        )
        .await
        .unwrap();
        assert!(matches!(result, CreateSpaceResult::Created(_)));
        append = Some(tokio::spawn(async move {
            append_operation(&append_pool, owner, &envelope, 1_000_000, 1_000_000).await
        }));
        waiting(&pool, &append_label).await;
    }
    gate.commit().await.unwrap();
    let deletion = tokio::time::timeout(Duration::from_secs(15), deletion)
        .await
        .unwrap()
        .unwrap();
    let creation = if let Some(result) = early {
        result
    } else {
        creation.await.unwrap()
    };
    let append = if let Some(append) = append {
        Some(append.await.unwrap())
    } else {
        None
    };
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER {trigger} ON security_spaces; DROP FUNCTION {trigger}();"
    )))
    .execute(&pool)
    .await
    .unwrap();
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE owner_user_id=$1")
            .bind(owner)
            .fetch_one(&pool)
            .await
            .unwrap();
    let deleted: bool = sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM users WHERE id=$1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .unwrap();
    drop_admission_database(&admin, pool, &schema).await;
    assert!(deletion.expect("production delete must not deadlock"));
    if let Some(append) = append {
        append.expect("production append must not deadlock");
    }
    assert!(
        blocked,
        "workspace creation must wait before admitting a new cascade"
    );
    assert_eq!(
        creation.unwrap_err().0,
        axum::http::StatusCode::UNAUTHORIZED
    );
    assert!(deleted);
    assert_eq!(remaining, 0);
}

#[tokio::test]
async fn final_workspace_admission_first_is_included_in_account_deletion() {
    let (admin, pool, schema) = admission_database().await;
    for (personal, existing) in [(false, false), (true, false), (true, true)] {
        let owner = Uuid::new_v4();
        let (workspace, _) = operation_fixture(&pool, owner).await;
        if !existing {
            sqlx::query("UPDATE workspaces SET kind='team' WHERE id=$1")
                .bind(workspace)
                .execute(&pool)
                .await
                .unwrap();
        }
        let (table, owner_column) = if existing {
            ("workspace_members", "user_id")
        } else {
            ("workspaces", "owner_user_id")
        };
        let trigger = format!("admit_first_{}", owner.simple());
        let gate_key = i64::from_be_bytes(owner.as_bytes()[..8].try_into().unwrap());
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.{owner_column} = '{owner}'::uuid THEN PERFORM pg_advisory_xact_lock({gate_key}); END IF; RETURN NEW; END; $$; CREATE TRIGGER {trigger} AFTER INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION {trigger}();"))).execute(&pool).await.unwrap();
        let create_label = format!("first-create-{owner}");
        let delete_label = format!("after-create-delete-{owner}");
        let create_pool = admission_pool(&schema, &create_label, 1).await;
        let delete_pool = admission_pool(&schema, &delete_label, 1).await;
        let mut gate = pool.begin().await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(gate_key)
            .execute(&mut *gate)
            .await
            .unwrap();
        let creation =
            tokio::spawn(async move { admit_workspace(&create_pool, owner, personal).await });
        waiting(&pool, &create_label).await;
        let deletion = tokio::spawn(async move { delete_account(&delete_pool, owner).await });
        waiting(&pool, &delete_label).await;
        let admission_wait = waits_for_admission(&pool, &delete_label, owner).await;
        gate.commit().await.unwrap();
        let (creation, deletion) = tokio::time::timeout(Duration::from_secs(15), async {
            tokio::join!(creation, deletion)
        })
        .await
        .unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "DROP TRIGGER {trigger} ON {table}; DROP FUNCTION {trigger}();"
        )))
        .execute(&pool)
        .await
        .unwrap();
        let remaining: i64 =
            sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE owner_user_id=$1")
                .bind(owner)
                .fetch_one(&pool)
                .await
                .unwrap();
        clean(&pool, owner).await;
        creation
            .unwrap()
            .expect("creation winning admission completes its membership atomically");
        assert!(
            deletion
                .unwrap()
                .expect("delete after creation must not deadlock")
        );
        assert!(
            admission_wait,
            "deletion must await complete workspace and membership admission"
        );
        assert_eq!(remaining, 0);
    }
    drop_admission_database(&admin, pool, &schema).await;
}

#[tokio::test]
async fn final_workspace_offer_and_account_deletion_share_admission() {
    let (admin, pool, schema) = admission_database().await;
    for deletion_first in [true, false] {
        let old = Uuid::new_v4();
        let recipient = Uuid::new_v4();
        let (workspace, _) = operation_fixture(&pool, old).await;
        operation_fixture(&pool, recipient).await;
        sqlx::query("UPDATE workspaces SET kind='team' WHERE id=$1")
            .bind(workspace)
            .execute(&pool)
            .await
            .unwrap();
        for (user, role) in [(old, "owner"), (recipient, "member")] {
            sqlx::query(
                "INSERT INTO workspace_members(id,workspace_id,user_id,role) VALUES($1,$2,$3,$4)",
            )
            .bind(Uuid::new_v4())
            .bind(workspace)
            .bind(user)
            .bind(role)
            .execute(&pool)
            .await
            .unwrap();
        }
        let offer = match create_offer(
            &pool,
            old,
            OwnershipResourceKind::Workspace,
            workspace,
            recipient,
        )
        .await
        .unwrap()
        {
            CreateOfferResult::Created(offer) => offer,
            _ => panic!("valid workspace transfer fixture"),
        };
        let accept_label = format!("admit-offer-{recipient}");
        let delete_label = format!("delete-offer-{recipient}");
        let accept_pool = admission_pool(&schema, &accept_label, 1).await;
        let delete_pool = admission_pool(&schema, &delete_label, 1).await;
        let trigger = format!("zz_offer_gate_{}", recipient.simple());
        let gate_key = i64::from_be_bytes(recipient.as_bytes()[..8].try_into().unwrap());
        if deletion_first {
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.owner_user_id = '{recipient}'::uuid THEN PERFORM pg_advisory_xact_lock({gate_key}); END IF; RETURN OLD; END; $$; CREATE TRIGGER {trigger} BEFORE DELETE ON security_spaces FOR EACH ROW EXECUTE FUNCTION {trigger}();"))).execute(&pool).await.unwrap();
        }
        let mut gate = pool.begin().await.unwrap();
        if deletion_first {
            sqlx::query("SELECT pg_advisory_xact_lock($1)")
                .bind(gate_key)
                .execute(&mut *gate)
                .await
                .unwrap();
        } else {
            sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR UPDATE")
                .bind(workspace)
                .execute(&mut *gate)
                .await
                .unwrap();
        }
        let accept = async move {
            accept_offer(
                &accept_pool,
                recipient,
                offer.transfer_id,
                1_000_000,
                1_000_000,
            )
            .await
        };
        let delete = async move { delete_account(&delete_pool, recipient).await };
        let (accept, delete, admission_wait) = if deletion_first {
            let delete = tokio::spawn(delete);
            waiting(&pool, &delete_label).await;
            let accept = tokio::spawn(accept);
            waiting(&pool, &accept_label).await;
            let admission_wait = waits_for_admission(&pool, &accept_label, recipient).await;
            (accept, delete, admission_wait)
        } else {
            let accept = tokio::spawn(accept);
            waiting(&pool, &accept_label).await;
            let delete = tokio::spawn(delete);
            waiting(&pool, &delete_label).await;
            let admission_wait = waits_for_admission(&pool, &delete_label, recipient).await;
            (accept, delete, admission_wait)
        };
        gate.commit().await.unwrap();
        let (accept, delete) = tokio::time::timeout(Duration::from_secs(15), async {
            tokio::join!(accept, delete)
        })
        .await
        .unwrap();
        if deletion_first {
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "DROP TRIGGER {trigger} ON security_spaces; DROP FUNCTION {trigger}();"
            )))
            .execute(&pool)
            .await
            .unwrap();
        }
        let owner: Uuid = sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id=$1")
            .bind(workspace)
            .fetch_one(&pool)
            .await
            .unwrap();
        // Remove the transferred workspace before its former space owner.
        clean(&pool, recipient).await;
        clean(&pool, old).await;
        let accept = accept
            .unwrap()
            .expect("workspace transfer must not deadlock against account deletion");
        let delete = delete
            .unwrap()
            .expect("account deletion must not deadlock against workspace transfer");
        assert!(
            admission_wait,
            "workspace admission must precede offer and resource locks"
        );
        if deletion_first {
            assert!(delete);
            assert!(matches!(
                accept,
                AcceptOfferResult::NotFound | AcceptOfferResult::NoLongerValid
            ));
            assert_eq!(owner, old);
        } else {
            assert!(matches!(accept, AcceptOfferResult::Accepted));
            assert!(
                !delete,
                "new shared workspace ownership must block account deletion"
            );
            assert_eq!(owner, recipient);
        }
    }
    drop_admission_database(&admin, pool, &schema).await;
}

#[tokio::test]
async fn final_admitted_invite_space_ownership_cannot_escape_account_deletion() {
    use crate::features::invites::repositories::{
        InviteCodeInsert, InviteCodeInsertResult, RedeemInviteOutcome, insert_invite_code,
        redeem_invite_code_tx,
    };
    use crate::features::spaces::dto::SpaceRole;
    let (admin, pool, schema) = admission_database().await;
    for deletion_first in [true, false] {
        let old = Uuid::new_v4();
        let recipient = Uuid::new_v4();
        let (_, envelope) = operation_fixture(&pool, old).await;
        let (_, own) = operation_fixture(&pool, recipient).await;
        for (owner, operation) in [(old, &envelope), (recipient, &own)] {
            append_operation(&pool, owner, operation, 1_000_000, 1_000_000)
                .await
                .unwrap();
        }
        let rotation = Uuid::new_v4();
        sqlx::query("INSERT INTO security_space_epochs(space_id,key_epoch,rotation_id,status,created_by,committed_at) VALUES($1,1,$2,'committed',$3,now())").bind(envelope.space_id).bind(rotation).bind(old).execute(&pool).await.unwrap();
        let mut hash = [0_u8; 32];
        hash[..16].copy_from_slice(Uuid::new_v4().as_bytes());
        assert!(matches!(
            insert_invite_code(
                &pool,
                InviteCodeInsert {
                    id: Uuid::new_v4(),
                    space_id: envelope.space_id,
                    rotation_id: rotation,
                    created_by: old,
                    role: SpaceRole::Editor,
                    code_hash: &hash,
                    encrypted_key_package: &[1],
                    encrypted_note: None,
                    ttl_minutes: 15,
                    request_hash: &[8; 32],
                }
            )
            .await
            .unwrap(),
            InviteCodeInsertResult::Stored(_)
        ));
        assert!(matches!(
            redeem_invite_code_tx(&pool, &hash, recipient)
                .await
                .unwrap(),
            RedeemInviteOutcome::Redeemed(_)
        ));
        let offer = match create_offer(
            &pool,
            old,
            OwnershipResourceKind::SecuritySpace,
            envelope.space_id,
            recipient,
        )
        .await
        .unwrap()
        {
            CreateOfferResult::Created(offer) => offer,
            _ => panic!("fresh invite member can receive a production ownership offer"),
        };
        let trigger = format!("zz_late_member_{}", recipient.simple());
        let gate_key = i64::from_be_bytes(recipient.as_bytes()[..8].try_into().unwrap());
        if deletion_first {
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.owner_user_id = '{recipient}'::uuid THEN PERFORM pg_advisory_xact_lock({gate_key}); END IF; RETURN OLD; END; $$; CREATE TRIGGER {trigger} BEFORE DELETE ON security_spaces FOR EACH ROW EXECUTE FUNCTION {trigger}();"))).execute(&pool).await.unwrap();
        }
        let mut gate = pool.begin().await.unwrap();
        if deletion_first {
            sqlx::query("SELECT pg_advisory_xact_lock($1)")
                .bind(gate_key)
                .execute(&mut *gate)
                .await
                .unwrap();
        } else {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,731))")
                .bind(recipient.to_string())
                .execute(&mut *gate)
                .await
                .unwrap();
        }
        let delete_label = format!("late-member-delete-{recipient}");
        let accept_label = format!("late-member-accept-{recipient}");
        let delete_pool = admission_pool(&schema, &delete_label, 1).await;
        let accept_pool = admission_pool(&schema, &accept_label, 1).await;
        let mut delete = None;
        if deletion_first {
            let delete_pool = delete_pool.clone();
            delete = Some(tokio::spawn(async move {
                delete_account(&delete_pool, recipient).await
            }));
            waiting(&pool, &delete_label).await;
        }
        // Ownership acceptance still races deletion after membership admission.
        let accept = tokio::spawn(async move {
            accept_offer(
                &accept_pool,
                recipient,
                offer.transfer_id,
                1_000_000,
                1_000_000,
            )
            .await
        });
        waiting(&pool, &accept_label).await;
        let admission_wait = if deletion_first {
            waits_for_admission(&pool, &accept_label, recipient).await
        } else {
            delete = Some(tokio::spawn(async move {
                delete_account(&delete_pool, recipient).await
            }));
            waiting(&pool, &delete_label).await;
            waits_for_admission(&pool, &delete_label, recipient).await
        };
        gate.commit().await.unwrap();
        let (accept, delete) = tokio::time::timeout(Duration::from_secs(15), async {
            tokio::join!(accept, delete.unwrap())
        })
        .await
        .unwrap();
        if deletion_first {
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "DROP TRIGGER {trigger} ON security_spaces; DROP FUNCTION {trigger}();"
            )))
            .execute(&pool)
            .await
            .unwrap();
        }
        let owner: Uuid =
            sqlx::query_scalar("SELECT owner_user_id FROM security_spaces WHERE id=$1")
                .bind(envelope.space_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let old_usage = usage(&pool, old).await;
        let recipient_usage = usage(&pool, recipient).await;
        let member_status: String = sqlx::query_scalar(
            "SELECT status FROM security_space_members WHERE space_id=$1 AND user_id=$2",
        )
        .bind(envelope.space_id)
        .bind(recipient)
        .fetch_one(&pool)
        .await
        .unwrap();
        clean(&pool, old).await;
        clean(&pool, recipient).await;
        let accept = accept
            .unwrap()
            .expect("late-member space ownership acceptance must not deadlock");
        let delete = delete
            .unwrap()
            .expect("account deletion must not deadlock against late-member ownership");
        assert!(
            admission_wait,
            "all ownership admission must acquire913 before offer/space/user locks"
        );
        if deletion_first {
            assert!(delete);
            assert!(matches!(
                accept,
                AcceptOfferResult::NotFound | AcceptOfferResult::NoLongerValid
            ));
            assert_eq!(owner, old);
            assert_eq!(member_status, "revoked");
            assert_eq!(old_usage, (282, 282));
            assert_eq!(recipient_usage, (0, 0));
        } else {
            assert!(matches!(accept, AcceptOfferResult::Accepted));
            assert!(
                !delete,
                "new shared space ownership blocks account deletion"
            );
            assert_eq!(owner, recipient);
            assert_eq!(old_usage, (0, 0));
            assert_eq!(recipient_usage, (564, 564));
        }
    }
    drop_admission_database(&admin, pool, &schema).await;
}

mod readiness_contention {
    use super::*;
    include!("readiness_contention.rs");
}
