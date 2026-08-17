//! Atomic, idempotent operation-log persistence.

use crypto_core_lib::operation_envelope::{EnvelopeCipherSuite, EnvelopeKind, OperationEnvelopeV1};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::dto::StoredOperation;

pub(crate) struct AppendAuthorization {
    pub(crate) signing_public_key: Vec<u8>,
    pub(crate) current_key_epoch: u32,
    pub(crate) can_write: bool,
}

pub(crate) enum AppendResult {
    Accepted(u64),
    Duplicate(u64),
    ConflictingDuplicate,
    AccessDeniedOrStaleEpoch,
}

pub(crate) async fn load_append_authorization(
    pool: &PgPool,
    user_id: Uuid,
    envelope: &OperationEnvelopeV1,
) -> anyhow::Result<Option<AppendAuthorization>> {
    let row = sqlx::query(
        r#"
        SELECT d.signing_public_key, s.current_key_epoch, sm.role
        FROM devices d
        JOIN security_spaces s ON s.id = $1 AND s.status = 'active'
        JOIN security_space_members sm
          ON sm.space_id = s.id
         AND sm.user_id = d.user_id
         AND sm.status = 'active'
        WHERE d.id = $2
          AND d.user_id = $3
          AND d.status = 'active'
        "#,
    )
    .bind(envelope.space_id)
    .bind(envelope.author_device_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        let epoch: i32 = row.try_get("current_key_epoch")?;
        let role: String = row.try_get("role")?;
        Ok(AppendAuthorization {
            signing_public_key: row.try_get("signing_public_key")?,
            current_key_epoch: u32::try_from(epoch)?,
            can_write: matches!(role.as_str(), "owner" | "editor"),
        })
    })
    .transpose()
}

pub(crate) async fn append_operation(
    pool: &PgPool,
    user_id: Uuid,
    envelope: &OperationEnvelopeV1,
) -> anyhow::Result<AppendResult> {
    let mut transaction = pool.begin().await?;

    if let Some(existing) = select_by_client_id(&mut transaction, envelope).await? {
        let result = if existing.envelope == *envelope {
            AppendResult::Duplicate(existing.space_seq)
        } else {
            AppendResult::ConflictingDuplicate
        };
        transaction.commit().await?;
        return Ok(result);
    }

    let next_sequence: Option<i64> = sqlx::query_scalar(
        r#"
        UPDATE security_spaces s
        SET next_sequence = next_sequence + 1
        WHERE s.id = $1
          AND s.status = 'active'
          AND s.current_key_epoch = $2
          AND EXISTS (
              SELECT 1
              FROM security_space_members sm
              WHERE sm.space_id = s.id
                AND sm.user_id = $3
                AND sm.status = 'active'
                AND sm.role IN ('owner', 'editor')
          )
          AND EXISTS (
              SELECT 1
              FROM devices d
              WHERE d.id = $4
                AND d.user_id = $3
                AND d.status = 'active'
          )
        RETURNING next_sequence
        "#,
    )
    .bind(envelope.space_id)
    .bind(i32::try_from(envelope.key_epoch)?)
    .bind(user_id)
    .bind(envelope.author_device_id)
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(next_sequence) = next_sequence else {
        transaction.rollback().await?;
        return Ok(AppendResult::AccessDeniedOrStaleEpoch);
    };

    let inserted: Option<i64> = sqlx::query_scalar(
        r#"
        INSERT INTO operation_log (
            space_id, space_seq, stream_id, client_op_id, author_device_id,
            key_epoch, envelope_kind, cipher_suite, nonce, ciphertext, signature
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT (space_id, client_op_id) DO NOTHING
        RETURNING space_seq
        "#,
    )
    .bind(envelope.space_id)
    .bind(next_sequence)
    .bind(envelope.stream_id)
    .bind(envelope.client_op_id)
    .bind(envelope.author_device_id)
    .bind(i32::try_from(envelope.key_epoch)?)
    .bind(envelope.envelope_kind.as_db_value())
    .bind(envelope.cipher_suite.as_db_value())
    .bind(&envelope.nonce)
    .bind(&envelope.ciphertext)
    .bind(&envelope.signature)
    .fetch_optional(&mut *transaction)
    .await?;

    let result = if let Some(sequence) = inserted {
        AppendResult::Accepted(u64::try_from(sequence)?)
    } else {
        let existing = select_by_client_id(&mut transaction, envelope)
            .await?
            .ok_or_else(|| anyhow::anyhow!("operation idempotency lookup failed"))?;
        if existing.envelope == *envelope {
            AppendResult::Duplicate(existing.space_seq)
        } else {
            AppendResult::ConflictingDuplicate
        }
    };
    transaction.commit().await?;
    Ok(result)
}

pub(crate) async fn list_operations(
    pool: &PgPool,
    user_id: Uuid,
    space_id: Uuid,
    since: u64,
    limit: u16,
) -> anyhow::Result<Option<Vec<StoredOperation>>> {
    let can_read: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM security_space_members sm
            JOIN security_spaces s ON s.id = sm.space_id
            WHERE sm.space_id = $1
              AND sm.user_id = $2
              AND sm.status = 'active'
              AND s.status = 'active'
        )
        "#,
    )
    .bind(space_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    if !can_read {
        return Ok(None);
    }

    let rows = sqlx::query(
        r#"
        SELECT space_seq, stream_id, client_op_id, author_device_id, key_epoch,
               envelope_kind, cipher_suite, nonce, ciphertext, signature,
               (extract(epoch FROM received_at) * 1000)::bigint AS received_at_ms
        FROM operation_log
        WHERE space_id = $1 AND space_seq > $2
        ORDER BY space_seq ASC
        LIMIT $3
        "#,
    )
    .bind(space_id)
    .bind(i64::try_from(since)?)
    .bind(i64::from(limit))
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| operation_from_row(space_id, row))
        .collect::<anyhow::Result<Vec<_>>>()
        .map(Some)
}

async fn select_by_client_id(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    envelope: &OperationEnvelopeV1,
) -> anyhow::Result<Option<StoredOperation>> {
    let row = sqlx::query(
        r#"
        SELECT space_seq, stream_id, client_op_id, author_device_id, key_epoch,
               envelope_kind, cipher_suite, nonce, ciphertext, signature,
               (extract(epoch FROM received_at) * 1000)::bigint AS received_at_ms
        FROM operation_log
        WHERE space_id = $1 AND client_op_id = $2
        "#,
    )
    .bind(envelope.space_id)
    .bind(envelope.client_op_id)
    .fetch_optional(&mut **transaction)
    .await?;
    row.as_ref()
        .map(|row| operation_from_row(envelope.space_id, row))
        .transpose()
}

fn operation_from_row(
    space_id: Uuid,
    row: &sqlx::postgres::PgRow,
) -> anyhow::Result<StoredOperation> {
    let key_epoch: i32 = row.try_get("key_epoch")?;
    let envelope_kind: String = row.try_get("envelope_kind")?;
    let cipher_suite: String = row.try_get("cipher_suite")?;
    let space_seq: i64 = row.try_get("space_seq")?;
    Ok(StoredOperation {
        space_seq: u64::try_from(space_seq)?,
        received_at_unix_ms: row.try_get("received_at_ms")?,
        envelope: OperationEnvelopeV1 {
            space_id,
            stream_id: row.try_get("stream_id")?,
            client_op_id: row.try_get("client_op_id")?,
            author_device_id: row.try_get("author_device_id")?,
            key_epoch: u32::try_from(key_epoch)?,
            envelope_kind: EnvelopeKind::from_db_value(&envelope_kind)?,
            cipher_suite: EnvelopeCipherSuite::from_db_value(&cipher_suite)?,
            nonce: row.try_get("nonce")?,
            ciphertext: row.try_get("ciphertext")?,
            signature: row.try_get("signature")?,
        },
    })
}
