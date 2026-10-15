// Finite release measurements against real repositories in disposable schemas.
use crate::features::invites::repositories::{
    InviteCodeInsert, InviteCodeInsertResult, RedeemInviteOutcome, insert_invite_code,
    redeem_invite_code_tx,
};
use crate::features::spaces::dto::SpaceRole;
use std::time::Instant;

async fn invite(pool: &PgPool, owner: Uuid, space: Uuid) -> [u8; 32] {
    let rotation = Uuid::new_v4();
    sqlx::query("INSERT INTO security_space_epochs(space_id,key_epoch,rotation_id,status,created_by,committed_at) VALUES($1,1,$2,'committed',$3,now())")
        .bind(space).bind(rotation).bind(owner).execute(pool).await.unwrap();
    let mut hash = [0; 32];
    hash[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    assert!(matches!(
        insert_invite_code(
            pool,
            InviteCodeInsert {
                id: Uuid::new_v4(),
                space_id: space,
                rotation_id: rotation,
                created_by: owner,
                role: SpaceRole::Editor,
                code_hash: &hash,
                encrypted_key_package: &[1],
                encrypted_note: None,
                ttl_minutes: 15,
                request_hash: &[8; 32]
            }
        )
        .await
        .unwrap(),
        InviteCodeInsertResult::Stored(_)
    ));
    hash
}

#[tokio::test]
async fn readiness_late_invite_after_membership_cleanup_cannot_revive_deleted_account() {
    use crate::features::common::authorize_session;
    let (admin, pool, schema) = admission_database().await;
    let owner = Uuid::new_v4();
    let recipient = Uuid::new_v4();
    let (_, envelope) = operation_fixture(&pool, owner).await;
    let (_, recipient_envelope) = operation_fixture(&pool, recipient).await;
    let hash = invite(&pool, owner, envelope.space_id).await;
    let session = Uuid::new_v4();
    sqlx::query("INSERT INTO refresh_tokens(id,user_id,token_hash,expires_at,device_id) VALUES($1,$2,$3,now()+interval '1 day',$4)").bind(session).bind(recipient).bind(hash.as_slice()).bind(recipient_envelope.author_device_id).execute(&pool).await.unwrap();
    let mut state = crate::platform::test_support::test_state();
    state.pool = pool.clone();
    state.account_state_checks_enabled = true;
    let token = state
        .issue_access_token(recipient, &format!("oplog-{recipient}"), session)
        .unwrap();
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    let trigger = format!("after_cleanup_{}", recipient.simple());
    let key = i64::from_be_bytes(recipient.as_bytes()[..8].try_into().unwrap());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.id='{recipient}'::uuid AND NEW.deleted_at IS NOT NULL THEN PERFORM pg_advisory_xact_lock({key}); END IF; RETURN NEW; END; $$; CREATE TRIGGER {trigger} BEFORE UPDATE ON users FOR EACH ROW EXECUTE FUNCTION {trigger}();"))).execute(&pool).await.unwrap();
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(key)
        .execute(&mut *gate)
        .await
        .unwrap();
    let delete_label = format!("cleanup-delete-{recipient}");
    let redeem_label = format!("cleanup-redeem-{recipient}");
    let deleting = admission_pool(&schema, &delete_label, 1).await;
    let redeeming = admission_pool(&schema, &redeem_label, 1).await;
    let delete = tokio::spawn(async move { delete_account(&deleting, recipient).await });
    waiting(&pool, &delete_label).await;
    // A genuine request can pass authorization while deletion is uncommitted.
    assert_eq!(
        authorize_session(&state, &headers).await.unwrap(),
        recipient
    );
    let mut redeem =
        tokio::spawn(async move { redeem_invite_code_tx(&redeeming, &hash, recipient).await });
    let early=tokio::time::timeout(Duration::from_secs(10),async {
        loop {tokio::select! {result=&mut redeem=>break Some(result), _=tokio::time::sleep(Duration::from_millis(10))=>{
            if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE application_name=$1 AND cardinality(pg_blocking_pids(pid))>0)").bind(&redeem_label).fetch_one(&pool).await.unwrap() {break None;}
        }}}
    }).await.unwrap();
    let blockers:Vec<(i32,Vec<i32>)>=sqlx::query_as("SELECT pid,pg_blocking_pids(pid) FROM pg_stat_activity WHERE application_name IN ($1,$2) ORDER BY application_name").bind(&delete_label).bind(&redeem_label).fetch_all(&pool).await.unwrap();
    let admission_wait = waits_for_admission(&pool, &redeem_label, recipient).await;
    gate.commit().await.unwrap();
    let (deleted, outcome) = tokio::time::timeout(Duration::from_secs(10), async {
        let deleted = delete.await.unwrap().unwrap();
        let outcome = if let Some(result) = early {
            result
        } else {
            redeem.await
        }
        .unwrap()
        .unwrap();
        (deleted, outcome)
    })
    .await
    .expect("deletion and late redemption must finish after gate release");
    assert!(deleted);
    let redeemed = matches!(outcome, RedeemInviteOutcome::Redeemed(_));
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM security_space_members WHERE user_id=$1 AND status='active'",
    )
    .bind(recipient)
    .fetch_one(&pool)
    .await
    .unwrap();
    let deleted: bool = sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM users WHERE id=$1")
        .bind(recipient)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(authorize_session(&state, &headers).await.is_err());
    println!(
        "READINESS late_invite blockers={blockers:?} redeemed={redeemed} deleted={deleted} active_memberships={active}"
    );
    drop(state);
    drop_admission_database(&admin, pool, &schema).await;
    assert!(deleted);
    assert!(
        admission_wait,
        "late redemption must wait on account913 before taking space/invite locks"
    );
    assert_eq!(
        active, 0,
        "successful account deletion must leave no active membership admitted after cleanup"
    );
    assert!(
        !redeemed,
        "deleted account must not receive a newly redeemed invite"
    );
}

fn distribution(name: &str, samples: &mut [u128]) {
    samples.sort_unstable();
    assert!(!samples.is_empty());
    let percentile = |p: usize| samples[(samples.len() * p).div_ceil(100).saturating_sub(1)];
    println!(
        "READINESS {name} samples={} min_us={} p50_us={} p95_us={} p99_us={} max_us={}",
        samples.len(),
        samples[0],
        percentile(50),
        percentile(95),
        percentile(99),
        samples[samples.len() - 1]
    );
}

#[tokio::test]
async fn readiness_sixteen_workers_preserve_exact_usage_and_sequences() {
    let (admin, pool, schema) = admission_database().await;
    let workers = admission_pool(&schema, "readiness-workload", 16).await;
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(16));
    let mut tasks = tokio::task::JoinSet::new();
    let mut accounts = Vec::new();
    for _ in 0..4 {
        let old = Uuid::new_v4();
        let recipient = Uuid::new_v4();
        let disposable = Uuid::new_v4();
        let (_, envelope) = operation_fixture(&pool, old).await;
        operation_fixture(&pool, recipient).await;
        operation_fixture(&pool, disposable).await;
        sqlx::query("INSERT INTO security_space_members(id,space_id,user_id,role,key_epoch) VALUES($1,$2,$3,'editor',1)").bind(Uuid::new_v4()).bind(envelope.space_id).bind(recipient).execute(&pool).await.unwrap();
        accounts.push((old, recipient, disposable, envelope.space_id));
        for kind in 0..4 {
            let pool = workers.clone();
            let barrier = barrier.clone();
            let envelope = envelope.clone();
            tasks.spawn(async move {
                barrier.wait().await;
                let mut samples=Vec::new();
                match kind {
                    0=>for _ in 0..16 {
                        let mut op=envelope.clone(); op.client_op_id=Uuid::new_v4(); op.sign(&SigningKey::from_bytes(&[17;32]));
                        let now=Instant::now();
                        let seq=match append_operation(&pool,old,&op,1_000_000_000,1_000_000_000).await.unwrap() {AppendResult::Accepted(seq)=>seq,_=>panic!("append must be accepted")}; samples.push(now.elapsed().as_micros());
                        let now=Instant::now();
                        assert!(matches!(append_operation(&pool,old,&op,1_000_000_000,1_000_000_000).await.unwrap(),AppendResult::Duplicate(s) if s==seq)); samples.push(now.elapsed().as_micros());
                        op.ciphertext=vec![99];op.sign(&SigningKey::from_bytes(&[17;32]));
                        let now=Instant::now();
                        assert!(matches!(append_operation(&pool,old,&op,1_000_000_000,1_000_000_000).await.unwrap(),AppendResult::ConflictingDuplicate)); samples.push(now.elapsed().as_micros());
                    },
                    1=>for _ in 0..16 {
                        let now=Instant::now();
                        assert!(matches!(store_blob(&pool,old,envelope.space_id,Uuid::new_v4(),&[7;32],256,1_000_000_000).await.unwrap(),StoreBlobResult::NeedsUpload(_))); samples.push(now.elapsed().as_micros());
                    },
                    2=>{
                        let now=Instant::now();
                        let offer=match create_offer(&pool,old,OwnershipResourceKind::SecuritySpace,envelope.space_id,recipient).await.unwrap() {CreateOfferResult::Created(offer)=>offer,_=>panic!("ownership offer")};samples.push(now.elapsed().as_micros());
                        let now=Instant::now();
                        assert!(matches!(accept_offer(&pool,recipient,offer.transfer_id,1_000_000_000,1_000_000_000).await.unwrap(),AcceptOfferResult::Accepted));samples.push(now.elapsed().as_micros());
                    },
                    _=>{
                        for _ in 0..8 {let now=Instant::now();admit_workspace(&pool,disposable,false).await.unwrap();samples.push(now.elapsed().as_micros());}
                        let now=Instant::now();assert!(delete_account(&pool,disposable).await.unwrap());samples.push(now.elapsed().as_micros());
                    }
                }
                (kind,samples)
            });
        }
    }
    let start = Instant::now();
    let mut samples = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(result) = tasks.join_next().await {
            let (kind, values) = result.unwrap();
            samples[kind].extend(values);
        }
    })
    .await
    .expect("finite workload must finish without deadlock");
    let elapsed = start.elapsed();
    for (old, recipient, disposable, space) in accounts {
        assert_eq!(usage(&pool, old).await, (0, 0));
        assert_eq!(usage(&pool, recipient).await, (4512, 4512));
        assert_eq!(usage(&pool, disposable).await, (0, 0));
        let sequence: Vec<i64> = sqlx::query_scalar(
            "SELECT space_seq FROM operation_log WHERE space_id=$1 ORDER BY space_seq",
        )
        .bind(space)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(sequence, (1..=16).collect::<Vec<_>>());
        let counters: (i64, i64) =
            sqlx::query_as("SELECT operation_bytes,next_sequence FROM security_spaces WHERE id=$1")
                .bind(space)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(counters, (4512, 16));
        let blobs:(i64,i64)=sqlx::query_as("SELECT (SELECT blob_storage_bytes FROM users WHERE id=$1),COALESCE(sum(size_padded),0)::bigint FROM space_blobs WHERE owner_user_id=$1").bind(recipient).fetch_one(&pool).await.unwrap();
        assert_eq!(blobs, (4096, 4096));
        let leftovers: i64 =
            sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE owner_user_id=$1")
                .bind(disposable)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(leftovers, 0);
    }
    let totals: (i64, i64) = sqlx::query_as(
        "SELECT sum(operation_bytes)::bigint,sum(blob_storage_bytes)::bigint FROM users",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        totals,
        (18_048, 16_384),
        "include old and deleted accounts when checking for leaked charges"
    );
    for (name, values) in [
        "append_with_retries",
        "blob",
        "ownership",
        "admission_delete",
    ]
    .into_iter()
    .zip(&mut samples)
    {
        distribution(name, values);
    }
    println!(
        "READINESS workload workers=16 groups=4 accounts=12 initial_spaces=12 transactions=300 accepted=64 duplicate=64 conflict=64 blob_reserved=64 offers=4 transfers=4 admitted=32 deleted=4 deadlocks=0 internal_errors=0 final_operation_bytes=18048 final_blob_bytes=16384 sequences=4x[1..16] elapsed_ms={}",
        elapsed.as_millis()
    );
    workers.close().await;
    drop_admission_database(&admin, pool, &schema).await;
}

#[tokio::test]
async fn readiness_invite_admitted_first_is_revoked_by_account_deletion() {
    let (admin, pool, schema) = admission_database().await;
    let owner = Uuid::new_v4();
    let recipient = Uuid::new_v4();
    let (_, envelope) = operation_fixture(&pool, owner).await;
    operation_fixture(&pool, recipient).await;
    let hash = invite(&pool, owner, envelope.space_id).await;
    let key = i64::from_be_bytes(recipient.as_bytes()[..8].try_into().unwrap());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION pause_admission() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.user_id='{recipient}'::uuid THEN PERFORM pg_advisory_xact_lock({key}); END IF; RETURN NEW; END; $$; CREATE TRIGGER pause_admission AFTER INSERT ON security_space_members FOR EACH ROW EXECUTE FUNCTION pause_admission();"))).execute(&pool).await.unwrap();
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(key)
        .execute(&mut *gate)
        .await
        .unwrap();
    let redeem_label = format!("first-redeem-{recipient}");
    let delete_label = format!("next-delete-{recipient}");
    let redeeming = admission_pool(&schema, &redeem_label, 1).await;
    let deleting = admission_pool(&schema, &delete_label, 1).await;
    let redeem =
        tokio::spawn(async move { redeem_invite_code_tx(&redeeming, &hash, recipient).await });
    waiting(&pool, &redeem_label).await;
    let delete = tokio::spawn(async move { delete_account(&deleting, recipient).await });
    waiting(&pool, &delete_label).await;
    assert!(waits_for_admission(&pool, &delete_label, recipient).await);
    let blockers:Vec<(i32,Vec<i32>)>=sqlx::query_as("SELECT pid,pg_blocking_pids(pid) FROM pg_stat_activity WHERE application_name IN ($1,$2) ORDER BY application_name").bind(&delete_label).bind(&redeem_label).fetch_all(&pool).await.unwrap();
    gate.commit().await.unwrap();
    let (redeem, delete) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(redeem, delete)
    })
    .await
    .unwrap();
    assert!(matches!(
        redeem.unwrap().unwrap(),
        RedeemInviteOutcome::Redeemed(_)
    ));
    assert!(delete.unwrap().unwrap());
    let status: String = sqlx::query_scalar(
        "SELECT status FROM security_space_members WHERE user_id=$1 AND space_id=$2",
    )
    .bind(recipient)
    .bind(envelope.space_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "revoked");
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM security_space_members WHERE user_id=$1 AND status='active'",
    )
    .bind(recipient)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active, 0);
    println!(
        "READINESS invite_first blockers={blockers:?} redeemed=true deleted=true member_status={status} active_memberships={active}"
    );
    drop_admission_database(&admin, pool, &schema).await;
}

#[tokio::test]
async fn readiness_large_account_usage_repair_bounds_live_writer_wait() {
    let admin = test_pool().await;
    let schema = format!("large_repair_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let pool = admission_pool(&schema, "repair-observer", 2).await;
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
    before.run(&pool).await.unwrap();
    let owner = Uuid::new_v4();
    let (_, mut envelope) = operation_fixture(&pool, owner).await;
    // Seed a finite historical account using the real signed append path.
    // Four spaces, 1000 operations each, 4096-byte ciphertext per operation.
    let workspace: Uuid =
        sqlx::query_scalar("SELECT workspace_id FROM security_spaces WHERE id=$1")
            .bind(envelope.space_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let seed_started = Instant::now();
    for index in 0..4 {
        if index > 0 {
            envelope.space_id = Uuid::new_v4();
            sqlx::query("INSERT INTO security_spaces(id,workspace_id,owner_user_id,created_by,encrypted_metadata) VALUES($1,$2,$3,$3,$4)").bind(envelope.space_id).bind(workspace).bind(owner).bind(vec![1_u8]).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO security_space_members(id,space_id,user_id,role,key_epoch) VALUES($1,$2,$3,'owner',1)").bind(Uuid::new_v4()).bind(envelope.space_id).bind(owner).execute(&pool).await.unwrap();
        }
        envelope.ciphertext = vec![11; 4096];
        for seq in 1..=1000 {
            envelope.client_op_id = Uuid::new_v4();
            envelope.sign(&SigningKey::from_bytes(&[17; 32]));
            assert!(
                matches!(append_operation(&pool,owner,&envelope,1_000_000_000,1_000_000_000).await.unwrap(),AppendResult::Accepted(s) if s==seq)
            );
        }
    }
    let seed_ms = seed_started.elapsed().as_millis();
    assert_eq!(usage(&pool, owner).await, (17_504_000, 17_504_000));
    sqlx::query("UPDATE security_spaces SET operation_bytes=1")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET operation_bytes=1")
        .execute(&pool)
        .await
        .unwrap();
    let migrating = admission_pool(&schema, "readiness-repair", 1).await;
    let writing = admission_pool(&schema, "readiness-repair-writer", 1).await;
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE operation_log IN SHARE MODE")
        .execute(&mut *gate)
        .await
        .unwrap();
    let migration = tokio::spawn(async move {
        let start = Instant::now();
        let result = MIGRATOR.run(&migrating).await;
        (result, start.elapsed())
    });
    wait_relation(
        &pool,
        "readiness-repair",
        &format!("{schema}.operation_log"),
    )
    .await;
    envelope.client_op_id = Uuid::new_v4();
    envelope.sign(&SigningKey::from_bytes(&[17; 32]));
    let append = tokio::spawn(async move {
        let start = Instant::now();
        let result =
            append_operation(&writing, owner, &envelope, 1_000_000_000, 1_000_000_000).await;
        (result, start.elapsed())
    });
    wait_relation(
        &pool,
        "readiness-repair-writer",
        &format!("{schema}.security_spaces"),
    )
    .await;
    let blockers:Vec<(i32,Vec<i32>)>=sqlx::query_as("SELECT pid,pg_blocking_pids(pid) FROM pg_stat_activity WHERE application_name IN ('readiness-repair','readiness-repair-writer') ORDER BY application_name").fetch_all(&pool).await.unwrap();
    let released = Instant::now();
    gate.commit().await.unwrap();
    let (migration, append) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(migration, append)
    })
    .await
    .unwrap();
    let (migration, migration_time) = migration.unwrap();
    migration.unwrap();
    let (append, writer_time) = append.unwrap();
    assert!(matches!(append.unwrap(), AppendResult::Accepted(1001)));
    let totals = usage(&pool, owner).await;
    assert_eq!(totals, (17_508_376, 17_508_376));
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*),count(DISTINCT (space_id,space_seq)),max(space_seq) FROM operation_log",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (4001, 4001, 1001));
    println!(
        "READINESS repair accounts=1 spaces=4 historical_rows=4000 ciphertext_bytes_per_row=4096 seed_ms={seed_ms} migration_ms={} writer_ms={} release_to_both_complete_ms={} blockers={blockers:?} final_operation_bytes={} final_rows=4001 unique_sequences=4001 max_sequence=1001",
        migration_time.as_millis(),
        writer_time.as_millis(),
        released.elapsed().as_millis(),
        totals.0
    );
    drop_admission_database(&admin, pool, &schema).await;
}

async fn wait_relation(pool: &PgPool, label: &str, relation: &str) {
    tokio::time::timeout(Duration::from_secs(10),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity a JOIN pg_locks l ON l.pid=a.pid WHERE a.application_name=$1 AND NOT l.granted AND l.locktype='relation' AND l.relation=to_regclass($2)::oid AND cardinality(pg_blocking_pids(a.pid))>0)").bind(label).bind(relation).fetch_one(pool).await.unwrap();
            if waiting {break;}tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("production transaction must reach the observed table lock");
}
