#[cfg(feature = "local-bridge")]
use super::types::DavChange;
use super::{DavResourceKind, LocalResource, UpsertOutcome};
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::operation_envelope::OperationEnvelopeV1;
use crate::pim::{PimBranchNodeV1, assign_pim_branches};
use sha2::{Digest, Sha256};

const DEFAULT_SYNC_SCOPE: &str = "workspace:personal";

#[derive(Clone, Debug)]
pub(crate) struct CachedOperationState {
    pub(crate) client_op_id: Uuid,
    /// Server-assigned transport sequence. Zero denotes a local operation not yet acknowledged.
    pub(crate) space_seq: u64,
    pub(crate) collection_id: String,
    pub(crate) stream_id: Uuid,
    pub(crate) logical_resource_id: String,
    pub(crate) materialized_resource_id: String,
    pub(crate) kind: DavResourceKind,
    pub(crate) payload: Option<String>,
    pub(crate) deleted: bool,
    pub(crate) parent_operation_id: Option<Uuid>,
    pub(crate) seed_projection_resource_id: Option<String>,
}

fn normalize_sync_scope(scope: &str) -> &str {
    let trimmed = scope.trim();
    if trimmed.is_empty() {
        DEFAULT_SYNC_SCOPE
    } else {
        trimmed
    }
}

/// SQLite cache access layer for decrypted DAV resources.
#[derive(Clone)]
pub(crate) struct LocalCache {
    db_path: PathBuf,
    db_key: Option<String>,
}

impl LocalCache {
    /// Creates cache handle and initializes schema.
    pub(crate) fn new(db_path: PathBuf, db_key: Option<String>) -> Result<Self> {
        let cache = Self {
            db_path,
            db_key: db_key.and_then(|value| {
                let trimmed = value.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }),
        };

        if let Err(error) = cache.init_schema() {
            if cache.db_key.is_some()
                && cache.db_path.exists()
                && cache.is_sqlcipher_mismatch_error(&error)
                && cache.can_open_as_plaintext_sqlite()
            {
                anyhow::bail!(
                    "unencrypted legacy cache detected at {}; delete it and resync, or export it explicitly before continuing",
                    cache.db_path.display()
                );
            }
            return Err(error);
        }

        Ok(cache)
    }

    /// Opens a SQLite connection configured for local bridge workload.
    fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("open sqlite db at {}", self.db_path.display()))?;
        if let Some(key) = self.db_key.as_deref() {
            conn.pragma_update(None, "key", key)
                .context("apply sqlcipher key")?;
            let _ = conn.pragma_update(None, "cipher_memory_security", 1_i64);
        }
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            "#,
        )?;
        Ok(conn)
    }

    /// Returns whether failure likely comes from SQLCipher key mismatch/opening mode mismatch.
    fn is_sqlcipher_mismatch_error(&self, error: &anyhow::Error) -> bool {
        let message = format!("{error:#}").to_ascii_lowercase();
        message.contains("file is not a database") || message.contains("file is encrypted")
    }

    /// Verifies whether DB is still readable as plaintext SQLite without any key.
    fn can_open_as_plaintext_sqlite(&self) -> bool {
        Connection::open(&self.db_path)
            .and_then(|conn| {
                conn.query_row("PRAGMA schema_version", [], |row| row.get::<_, i64>(0))
            })
            .is_ok()
    }

    /// Creates required local tables for contacts/calendars/notes and sync metadata.
    fn init_schema(&self) -> Result<()> {
        let conn = self.connect()?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS contacts (
                collection_id TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                vcard TEXT NOT NULL,
                etag TEXT NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                PRIMARY KEY (collection_id, resource_id)
            );

            CREATE TABLE IF NOT EXISTS calendars (
                collection_id TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                ical TEXT NOT NULL,
                etag TEXT NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                PRIMARY KEY (collection_id, resource_id)
            );

            CREATE TABLE IF NOT EXISTS notes (
                collection_id TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                note_text TEXT NOT NULL,
                etag TEXT NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                PRIMARY KEY (collection_id, resource_id)
            );

            CREATE TABLE IF NOT EXISTS sync_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                last_seq_id INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sync_cursors (
                scope TEXT PRIMARY KEY,
                last_seq_id INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS resource_heads (
                collection_id TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                client_op_id TEXT NOT NULL,
                PRIMARY KEY (collection_id, resource_id)
            );

            CREATE TABLE IF NOT EXISTS operation_states (
                collection_id TEXT NOT NULL,
                client_op_id TEXT NOT NULL,
                stream_id TEXT NOT NULL,
                logical_resource_id TEXT NOT NULL,
                materialized_resource_id TEXT NOT NULL,
                resource_kind TEXT NOT NULL,
                payload TEXT,
                deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
                space_seq INTEGER NOT NULL DEFAULT 0,
                parent_operation_id TEXT,
                seed_projection_resource_id TEXT,
                PRIMARY KEY (collection_id, client_op_id)
            );

            CREATE INDEX IF NOT EXISTS idx_operation_states_resource
                ON operation_states (collection_id, logical_resource_id);

            CREATE TABLE IF NOT EXISTS operation_outbox (
                space_id TEXT NOT NULL,
                client_op_id TEXT NOT NULL,
                envelope BLOB NOT NULL,
                created_at_ms INTEGER NOT NULL,
                queue_order INTEGER NOT NULL,
                PRIMARY KEY (space_id, client_op_id)
            );

            CREATE TABLE IF NOT EXISTS quarantined_operations (
                space_id TEXT NOT NULL,
                client_op_id TEXT NOT NULL,
                space_seq INTEGER NOT NULL,
                reason_code TEXT NOT NULL,
                envelope BLOB NOT NULL,
                quarantined_at_ms INTEGER NOT NULL,
                PRIMARY KEY (space_id, client_op_id)
            );

            CREATE TABLE IF NOT EXISTS dav_changes (
                revision INTEGER PRIMARY KEY AUTOINCREMENT,
                resource_kind TEXT NOT NULL,
                collection_id TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                etag TEXT,
                deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
            );

            CREATE INDEX IF NOT EXISTS idx_dav_changes_collection_revision
                ON dav_changes (resource_kind, collection_id, revision);

            CREATE TABLE IF NOT EXISTS runtime_credentials (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                access_token TEXT NOT NULL,
                refresh_token TEXT NOT NULL,
                previous_refresh_hash BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS refresh_rotation_attempt (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                refresh_token_hash BLOB NOT NULL,
                rotation_request_id TEXT NOT NULL
            );

            INSERT OR IGNORE INTO sync_state (id, last_seq_id) VALUES (1, 0);
            INSERT INTO sync_cursors (scope, last_seq_id)
            SELECT 'workspace:personal', last_seq_id
            FROM sync_state
            WHERE id = 1
            ON CONFLICT(scope) DO NOTHING;
            "#,
        )?;
        ensure_column(
            &conn,
            "operation_states",
            "stream_id",
            "ALTER TABLE operation_states ADD COLUMN stream_id TEXT",
        )?;
        ensure_column(
            &conn,
            "operation_states",
            "parent_operation_id",
            "ALTER TABLE operation_states ADD COLUMN parent_operation_id TEXT",
        )?;
        ensure_column(
            &conn,
            "operation_states",
            "seed_projection_resource_id",
            "ALTER TABLE operation_states ADD COLUMN seed_projection_resource_id TEXT",
        )?;
        ensure_column(
            &conn,
            "operation_states",
            "space_seq",
            "ALTER TABLE operation_states ADD COLUMN space_seq INTEGER NOT NULL DEFAULT 0",
        )?;
        conn.execute(
            "UPDATE operation_states SET stream_id = logical_resource_id WHERE stream_id IS NULL",
            [],
        )?;
        migrate_operation_state_scope(&conn)?;
        migrate_operation_outbox_scope(&conn)?;
        ensure_column(
            &conn,
            "operation_outbox",
            "queue_order",
            "ALTER TABLE operation_outbox ADD COLUMN queue_order INTEGER NOT NULL DEFAULT 0",
        )?;
        conn.execute(
            "UPDATE operation_outbox SET queue_order = rowid WHERE queue_order = 0",
            [],
        )?;
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_operation_outbox_queue_order ON operation_outbox (queue_order)",
            [],
        )?;
        Ok(())
    }

    /// Recovers a refresh rotation only when the caller supplied either the
    /// stored token or its immediate predecessor. This prevents credentials
    /// from a previous account on the same backend/cache path being reused.
    pub(crate) fn recover_rotated_credentials(
        &self,
        supplied_refresh_token: Option<&str>,
    ) -> Result<Option<(String, String)>> {
        let Some(supplied) = supplied_refresh_token else {
            return Ok(None);
        };
        let row = self
            .connect()?
            .query_row(
                "SELECT access_token, refresh_token, previous_refresh_hash FROM runtime_credentials WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((access_token, refresh_token, previous_hash)) = row else {
            return Ok(None);
        };
        let supplied_hash = Sha256::digest(supplied.as_bytes());
        if refresh_token == supplied || supplied_hash.as_slice() == previous_hash.as_slice() {
            Ok(Some((access_token, refresh_token)))
        } else {
            Ok(None)
        }
    }

    /// Durably commits a rotated credential pair before the protected request
    /// is retried. The cache is SQLCipher-protected whenever auth persistence is
    /// enabled by desktop/mobile clients.
    pub(crate) fn store_rotated_credentials(
        &self,
        previous_refresh_token: &str,
        access_token: &str,
        refresh_token: &str,
    ) -> Result<()> {
        anyhow::ensure!(
            self.db_key.is_some(),
            "encrypted cache is required for token rotation"
        );
        let mut conn = self.connect()?;
        let transaction = conn.transaction()?;
        transaction.execute(
            r#"
            INSERT INTO runtime_credentials (
                id, access_token, refresh_token, previous_refresh_hash
            ) VALUES (1, ?1, ?2, ?3)
            ON CONFLICT(id) DO UPDATE SET
                access_token = excluded.access_token,
                refresh_token = excluded.refresh_token,
                previous_refresh_hash = excluded.previous_refresh_hash
            "#,
            params![
                access_token,
                refresh_token,
                Sha256::digest(previous_refresh_token.as_bytes()).as_slice(),
            ],
        )?;
        transaction.execute(
            "DELETE FROM refresh_rotation_attempt WHERE id = 1 AND refresh_token_hash = ?1",
            params![Sha256::digest(previous_refresh_token.as_bytes()).as_slice()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Returns one crash-stable random identity for rotating this exact token.
    /// The row is committed before any network request is sent and is retained
    /// until the matching replacement credentials are durably committed.
    pub(crate) fn begin_refresh_rotation(&self, refresh_token: &str) -> Result<Uuid> {
        anyhow::ensure!(
            self.db_key.is_some(),
            "encrypted cache is required for token rotation"
        );
        let token_hash = Sha256::digest(refresh_token.as_bytes()).to_vec();
        let mut conn = self.connect()?;
        let transaction = conn.transaction()?;
        let existing: Option<(Vec<u8>, String)> = transaction
            .query_row(
                "SELECT refresh_token_hash, rotation_request_id FROM refresh_rotation_attempt WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((stored_hash, request_id)) = existing
            && stored_hash == token_hash
        {
            let request_id = Uuid::parse_str(&request_id)
                .context("parse persisted refresh rotation request id")?;
            transaction.commit()?;
            return Ok(request_id);
        }

        let request_id = Uuid::new_v4();
        transaction.execute(
            r#"
            INSERT INTO refresh_rotation_attempt (id, refresh_token_hash, rotation_request_id)
            VALUES (1, ?1, ?2)
            ON CONFLICT(id) DO UPDATE SET
                refresh_token_hash = excluded.refresh_token_hash,
                rotation_request_id = excluded.rotation_request_id
            "#,
            params![token_hash, request_id.to_string()],
        )?;
        transaction.commit()?;
        Ok(request_id)
    }

    pub(crate) fn clear_runtime_credentials(&self) -> Result<()> {
        let mut conn = self.connect()?;
        let transaction = conn.transaction()?;
        transaction.execute("DELETE FROM runtime_credentials WHERE id = 1", [])?;
        transaction.execute("DELETE FROM refresh_rotation_attempt WHERE id = 1", [])?;
        transaction.commit()?;
        Ok(())
    }

    /// Inserts/updates a resource using LWW timestamp semantics.
    #[cfg(test)]
    pub(crate) fn upsert_lww(&self, resource: &LocalResource) -> Result<UpsertOutcome> {
        let mut conn = self.connect()?;
        let transaction = conn.transaction()?;
        let table = resource.kind.table_name();
        let payload_col = resource.kind.payload_column();

        let select_sql = format!(
            "SELECT updated_at_ms FROM {table} WHERE collection_id = ?1 AND resource_id = ?2"
        );
        let current_ts: Option<i64> = transaction
            .query_row(
                &select_sql,
                params![resource.collection_id, resource.resource_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;

        if let Some(existing) = current_ts {
            if resource.updated_at_ms < existing {
                return Ok(UpsertOutcome::IgnoredStale);
            }
            let update_sql = format!(
                "UPDATE {table} SET {payload_col} = ?1, etag = ?2, updated_at_ms = ?3 WHERE collection_id = ?4 AND resource_id = ?5"
            );
            transaction.execute(
                &update_sql,
                params![
                    resource.payload,
                    resource.etag,
                    resource.updated_at_ms,
                    resource.collection_id,
                    resource.resource_id
                ],
            )?;
            record_dav_change(&transaction, resource, false)?;
            transaction.commit()?;
            return Ok(UpsertOutcome::Updated);
        }

        let insert_sql = format!(
            "INSERT INTO {table} (collection_id, resource_id, {payload_col}, etag, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)"
        );
        transaction.execute(
            &insert_sql,
            params![
                resource.collection_id,
                resource.resource_id,
                resource.payload,
                resource.etag,
                resource.updated_at_ms
            ],
        )?;
        record_dav_change(&transaction, resource, false)?;
        transaction.commit()?;
        Ok(UpsertOutcome::Inserted)
    }

    /// Applies an operation already ordered and authorized by the cloud log.
    /// Local wall-clock values are intentionally ignored because they are not
    /// comparable with server sequence cursors.
    pub(crate) fn upsert_authoritative(&self, resource: &LocalResource) -> Result<UpsertOutcome> {
        let mut conn = self.connect()?;
        let transaction = conn.transaction()?;
        let table = resource.kind.table_name();
        let payload_col = resource.kind.payload_column();
        let exists_sql = format!(
            "SELECT EXISTS(SELECT 1 FROM {table} WHERE collection_id = ?1 AND resource_id = ?2)"
        );
        let exists: bool = transaction.query_row(
            &exists_sql,
            params![resource.collection_id, resource.resource_id],
            |row| row.get(0),
        )?;
        let sql = format!(
            "INSERT INTO {table} (collection_id, resource_id, {payload_col}, etag, updated_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(collection_id, resource_id) DO UPDATE SET \
             {payload_col} = excluded.{payload_col}, etag = excluded.etag, \
             updated_at_ms = excluded.updated_at_ms"
        );
        transaction.execute(
            &sql,
            params![
                resource.collection_id,
                resource.resource_id,
                resource.payload,
                resource.etag,
                resource.updated_at_ms
            ],
        )?;
        record_dav_change(&transaction, resource, false)?;
        transaction.commit()?;
        Ok(if exists {
            UpsertOutcome::Updated
        } else {
            UpsertOutcome::Inserted
        })
    }

    /// Returns all resources for the specified collection/kind.
    pub(crate) fn list_resources(
        &self,
        kind: DavResourceKind,
        collection_id: &str,
    ) -> Result<Vec<LocalResource>> {
        let conn = self.connect()?;
        let table = kind.table_name();
        let payload_col = kind.payload_column();
        let query = format!(
            "SELECT resource_id, {payload_col}, etag, updated_at_ms FROM {table} WHERE collection_id = ?1 ORDER BY resource_id"
        );

        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(params![collection_id], |row| {
            Ok(LocalResource {
                kind,
                collection_id: collection_id.to_string(),
                resource_id: row.get(0)?,
                payload: row.get(1)?,
                etag: row.get(2)?,
                updated_at_ms: row.get(3)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Returns a single resource row for the specified key.
    pub(crate) fn get_resource(
        &self,
        kind: DavResourceKind,
        collection_id: &str,
        resource_id: &str,
    ) -> Result<Option<LocalResource>> {
        let conn = self.connect()?;
        let table = kind.table_name();
        let payload_col = kind.payload_column();
        let query = format!(
            "SELECT {payload_col}, etag, updated_at_ms FROM {table} WHERE collection_id = ?1 AND resource_id = ?2"
        );
        let row = conn
            .query_row(&query, params![collection_id, resource_id], |row| {
                Ok(LocalResource {
                    kind,
                    collection_id: collection_id.to_string(),
                    resource_id: resource_id.to_string(),
                    payload: row.get(0)?,
                    etag: row.get(1)?,
                    updated_at_ms: row.get(2)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    /// Reads `sync_cursors.last_seq_id` for a workspace scope.
    pub(crate) fn load_last_seq(&self, scope: &str) -> Result<u64> {
        let scope = normalize_sync_scope(scope);
        let conn = self.connect()?;
        let value: Option<i64> = conn
            .query_row(
                "SELECT last_seq_id FROM sync_cursors WHERE scope = ?1",
                params![scope],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(value) = value {
            return Ok(value.max(0) as u64);
        }

        if scope == DEFAULT_SYNC_SCOPE {
            let legacy_value: i64 = conn.query_row(
                "SELECT last_seq_id FROM sync_state WHERE id = 1",
                [],
                |row| row.get(0),
            )?;
            return Ok(legacy_value.max(0) as u64);
        }

        Ok(0)
    }

    /// Upserts `sync_cursors.last_seq_id` for a workspace scope.
    pub(crate) fn store_last_seq(&self, scope: &str, seq: u64) -> Result<()> {
        let scope = normalize_sync_scope(scope);
        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO sync_cursors (scope, last_seq_id)
            VALUES (?1, ?2)
            ON CONFLICT(scope) DO UPDATE SET last_seq_id = excluded.last_seq_id
            "#,
            params![scope, seq as i64],
        )?;
        if scope == DEFAULT_SYNC_SCOPE {
            conn.execute(
                "UPDATE sync_state SET last_seq_id = ?1 WHERE id = 1",
                params![seq as i64],
            )?;
        }
        Ok(())
    }

    /// Raises a cursor to an authenticated membership boundary without ever
    /// moving an existing local cursor backwards.
    pub(crate) fn advance_last_seq(&self, scope: &str, seq: u64) -> Result<()> {
        let scope = normalize_sync_scope(scope);
        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO sync_cursors (scope, last_seq_id)
            VALUES (?1, ?2)
            ON CONFLICT(scope) DO UPDATE SET
                last_seq_id = MAX(sync_cursors.last_seq_id, excluded.last_seq_id)
            "#,
            params![scope, i64::try_from(seq).unwrap_or(i64::MAX)],
        )?;
        Ok(())
    }

    pub(crate) fn load_resource_head(
        &self,
        collection_id: &str,
        resource_id: &str,
    ) -> Result<Option<Uuid>> {
        let conn = self.connect()?;
        let value: Option<String> = conn
            .query_row(
                "SELECT client_op_id FROM resource_heads WHERE collection_id = ?1 AND resource_id = ?2",
                params![collection_id, resource_id],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|value| Uuid::parse_str(&value).context("invalid cached operation head"))
            .transpose()
    }

    pub(crate) fn load_operation_state(
        &self,
        collection_id: &str,
        client_op_id: Uuid,
    ) -> Result<Option<CachedOperationState>> {
        let conn = self.connect()?;
        conn.query_row(
            r#"
            SELECT client_op_id, collection_id, stream_id, logical_resource_id,
                   materialized_resource_id, resource_kind, payload, deleted, space_seq,
                   parent_operation_id, seed_projection_resource_id
            FROM operation_states WHERE collection_id = ?1 AND client_op_id = ?2
            "#,
            params![collection_id, client_op_id.to_string()],
            map_operation_state,
        )
        .optional()
        .map_err(Into::into)
    }

    pub(crate) fn store_operation_state(&self, state: &CachedOperationState) -> Result<()> {
        let conn = self.connect()?;
        upsert_operation_state(&conn, state)?;
        Ok(())
    }

    /// Commits an operation and its materialized resource head together.
    ///
    /// Replay treats a cached operation as already applied. The matching head must
    /// therefore become visible in the same SQLite transaction, including after a
    /// process crash between local materialization and cloud acknowledgement.
    pub(crate) fn store_operation_state_and_head(
        &self,
        state: &CachedOperationState,
        resource_id: &str,
    ) -> Result<()> {
        let mut conn = self.connect()?;
        let transaction = conn.transaction()?;
        upsert_operation_state(&transaction, state)?;
        transaction.execute(
            r#"
            INSERT INTO resource_heads (collection_id, resource_id, client_op_id)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(collection_id, resource_id) DO UPDATE SET
                client_op_id = excluded.client_op_id
            "#,
            params![
                state.collection_id,
                resource_id,
                state.client_op_id.to_string()
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn acknowledge_operation(
        &self,
        collection_id: &str,
        client_op_id: Uuid,
        space_seq: u64,
    ) -> Result<()> {
        self.connect()?.execute(
            "UPDATE operation_states SET space_seq = MAX(space_seq, ?3) WHERE collection_id = ?1 AND client_op_id = ?2",
            params![
                collection_id,
                client_op_id.to_string(),
                i64::try_from(space_seq).unwrap_or(i64::MAX)
            ],
        )?;
        Ok(())
    }

    pub(crate) fn list_materialized_head_states(
        &self,
        collection_id: &str,
    ) -> Result<Vec<CachedOperationState>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            r#"
            SELECT state.client_op_id, state.collection_id, state.stream_id,
                   state.logical_resource_id, state.materialized_resource_id,
                   state.resource_kind, state.payload, state.deleted, state.space_seq,
                   state.parent_operation_id, state.seed_projection_resource_id
            FROM operation_states state
            JOIN resource_heads head
              ON head.collection_id = state.collection_id
             AND head.resource_id = state.materialized_resource_id
             AND head.client_op_id = state.client_op_id
            WHERE state.collection_id = ?1
            ORDER BY state.stream_id, state.materialized_resource_id
            "#,
        )?;
        statement
            .query_map(params![collection_id], map_operation_state)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Rebuilds one stream's projection rows from its complete known version
    /// graph in a single transaction. Arrival order cannot influence branch
    /// identity or which branch inherits the canonical DAV resource name.
    pub(crate) fn reconcile_pim_stream(
        &self,
        collection_id: &str,
        stream_id: Uuid,
        default_projection_resource_id: &str,
        kind: DavResourceKind,
    ) -> Result<()> {
        let mut conn = self.connect()?;
        let transaction = conn.transaction()?;
        let states = {
            let mut statement = transaction.prepare(
                r#"
                SELECT client_op_id, collection_id, stream_id, logical_resource_id,
                       materialized_resource_id, resource_kind, payload, deleted, space_seq,
                       parent_operation_id, seed_projection_resource_id
                FROM operation_states
                WHERE collection_id = ?1 AND stream_id = ?2
                "#,
            )?;
            statement
                .query_map(
                    params![collection_id, stream_id.to_string()],
                    map_operation_state,
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let nodes = states
            .iter()
            .map(|state| PimBranchNodeV1 {
                operation_id: state.client_op_id,
                parent_operation_id: state.parent_operation_id,
                seed_projection_resource_id: state.seed_projection_resource_id.clone(),
            })
            .collect::<Vec<_>>();
        let assignments = assign_pim_branches(default_projection_resource_id, &nodes)?;
        let assignment_by_id = assignments
            .iter()
            .map(|assignment| (assignment.operation_id, assignment))
            .collect::<std::collections::HashMap<_, _>>();

        let previous_projection_ids = states
            .iter()
            .map(|state| state.materialized_resource_id.clone())
            .collect::<std::collections::HashSet<_>>();
        let desired_projection_ids = assignments
            .iter()
            .filter(|assignment| assignment.head)
            .map(|assignment| assignment.projection_resource_id.clone())
            .collect::<std::collections::HashSet<_>>();
        let table = kind.table_name();
        for projection_id in previous_projection_ids.difference(&desired_projection_ids) {
            let deleted = transaction.execute(
                &format!("DELETE FROM {table} WHERE collection_id = ?1 AND resource_id = ?2"),
                params![collection_id, projection_id],
            )?;
            transaction.execute(
                "DELETE FROM resource_heads WHERE collection_id = ?1 AND resource_id = ?2",
                params![collection_id, projection_id],
            )?;
            if deleted > 0 {
                transaction.execute(
                    "INSERT INTO dav_changes (resource_kind, collection_id, resource_id, etag, deleted) VALUES (?1, ?2, ?3, NULL, 1)",
                    params![kind.route_prefix(), collection_id, projection_id],
                )?;
            }
        }

        for state in &states {
            let assignment = assignment_by_id
                .get(&state.client_op_id)
                .ok_or_else(|| anyhow::anyhow!("missing PIM branch assignment"))?;
            transaction.execute(
                "UPDATE operation_states SET materialized_resource_id = ?3 WHERE collection_id = ?1 AND client_op_id = ?2",
                params![
                    collection_id,
                    state.client_op_id.to_string(),
                    assignment.projection_resource_id,
                ],
            )?;
            if !assignment.head {
                continue;
            }
            transaction.execute(
                r#"
                INSERT INTO resource_heads (collection_id, resource_id, client_op_id)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(collection_id, resource_id) DO UPDATE SET
                    client_op_id = excluded.client_op_id
                "#,
                params![
                    collection_id,
                    assignment.projection_resource_id,
                    state.client_op_id.to_string(),
                ],
            )?;
            let Some(payload) = state.payload.as_deref().filter(|_| !state.deleted) else {
                let deleted = transaction.execute(
                    &format!("DELETE FROM {table} WHERE collection_id = ?1 AND resource_id = ?2"),
                    params![collection_id, assignment.projection_resource_id],
                )?;
                if deleted > 0 {
                    transaction.execute(
                        "INSERT INTO dav_changes (resource_kind, collection_id, resource_id, etag, deleted) VALUES (?1, ?2, ?3, NULL, 1)",
                        params![kind.route_prefix(), collection_id, assignment.projection_resource_id],
                    )?;
                }
                continue;
            };
            let payload_column = kind.payload_column();
            let etag = hex::encode(Sha256::digest(payload.as_bytes()));
            let existing: Option<(String, String)> = transaction
                .query_row(
                    &format!(
                        "SELECT {payload_column}, etag FROM {table} WHERE collection_id = ?1 AND resource_id = ?2"
                    ),
                    params![collection_id, assignment.projection_resource_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if existing
                .as_ref()
                .is_some_and(|(existing_payload, existing_etag)| {
                    existing_payload == payload && existing_etag == &etag
                })
            {
                continue;
            }
            transaction.execute(
                &format!(
                    "INSERT INTO {table} (collection_id, resource_id, {payload_column}, etag, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(collection_id, resource_id) DO UPDATE SET
                       {payload_column} = excluded.{payload_column},
                       etag = excluded.etag,
                       updated_at_ms = excluded.updated_at_ms"
                ),
                params![
                    collection_id,
                    assignment.projection_resource_id,
                    payload,
                    etag,
                    i64::try_from(state.space_seq).unwrap_or(i64::MAX),
                ],
            )?;
            transaction.execute(
                "INSERT INTO dav_changes (resource_kind, collection_id, resource_id, etag, deleted) VALUES (?1, ?2, ?3, ?4, 0)",
                params![kind.route_prefix(), collection_id, assignment.projection_resource_id, etag],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn delete_resource(
        &self,
        kind: DavResourceKind,
        collection_id: &str,
        resource_id: &str,
    ) -> Result<bool> {
        let mut conn = self.connect()?;
        let transaction = conn.transaction()?;
        let deleted = transaction.execute(
            &format!(
                "DELETE FROM {} WHERE collection_id = ?1 AND resource_id = ?2",
                kind.table_name()
            ),
            params![collection_id, resource_id],
        )?;
        if deleted == 1 {
            transaction.execute(
                "INSERT INTO dav_changes (resource_kind, collection_id, resource_id, etag, deleted) VALUES (?1, ?2, ?3, NULL, 1)",
                params![kind.route_prefix(), collection_id, resource_id],
            )?;
        }
        transaction.commit()?;
        Ok(deleted == 1)
    }

    #[cfg(feature = "local-bridge")]
    pub(crate) fn latest_dav_revision(
        &self,
        kind: DavResourceKind,
        collection_id: &str,
    ) -> Result<u64> {
        let revision: Option<i64> = self.connect()?.query_row(
            "SELECT MAX(revision) FROM dav_changes WHERE resource_kind = ?1 AND collection_id = ?2",
            params![kind.route_prefix(), collection_id],
            |row| row.get(0),
        )?;
        Ok(revision.unwrap_or(0).max(0) as u64)
    }

    #[cfg(feature = "local-bridge")]
    pub(crate) fn list_dav_changes_since(
        &self,
        kind: DavResourceKind,
        collection_id: &str,
        revision: u64,
    ) -> Result<Vec<DavChange>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            r#"
            SELECT revision, resource_id, deleted
            FROM dav_changes
            WHERE resource_kind = ?1 AND collection_id = ?2 AND revision > ?3
            ORDER BY revision ASC
            "#,
        )?;
        let rows = statement.query_map(
            params![kind.route_prefix(), collection_id, revision as i64],
            |row| {
                Ok(DavChange {
                    revision: row.get::<_, i64>(0)?.max(0) as u64,
                    resource_id: row.get(1)?,
                    deleted: row.get::<_, i64>(2)? != 0,
                })
            },
        )?;
        let mut changes = Vec::new();
        for row in rows {
            changes.push(row?);
        }
        Ok(changes)
    }

    pub(crate) fn queue_operation(
        &self,
        envelope: &OperationEnvelopeV1,
        created_at_ms: i64,
    ) -> Result<()> {
        let encoded = rmp_serde::to_vec_named(envelope).context("encode operation outbox item")?;
        let mut conn = self.connect()?;
        let transaction = conn.transaction()?;
        let queue_order: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(queue_order), 0) + 1 FROM operation_outbox",
            [],
            |row| row.get(0),
        )?;
        transaction.execute(
            r#"
            INSERT INTO operation_outbox (
                space_id, client_op_id, envelope, created_at_ms, queue_order
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(space_id, client_op_id) DO UPDATE SET
                envelope = excluded.envelope,
                created_at_ms = excluded.created_at_ms
            "#,
            params![
                envelope.space_id.to_string(),
                envelope.client_op_id.to_string(),
                encoded,
                created_at_ms,
                queue_order
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn list_queued_operations(&self) -> Result<Vec<OperationEnvelopeV1>> {
        let conn = self.connect()?;
        let mut statement =
            conn.prepare("SELECT envelope FROM operation_outbox ORDER BY queue_order ASC")?;
        let rows = statement.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
        let mut operations = Vec::new();
        for row in rows {
            operations.push(rmp_serde::from_slice(&row?).context("decode operation outbox item")?);
        }
        Ok(operations)
    }

    pub(crate) fn remove_queued_operation(&self, space_id: Uuid, client_op_id: Uuid) -> Result<()> {
        self.connect()?.execute(
            "DELETE FROM operation_outbox WHERE space_id = ?1 AND client_op_id = ?2",
            params![space_id.to_string(), client_op_id.to_string()],
        )?;
        Ok(())
    }

    /// Persists an authenticated but unusable envelope so one poisoned entry
    /// cannot permanently block the space cursor. The local database is
    /// encrypted in production and no decrypted payload is retained here.
    pub(crate) fn quarantine_operation(
        &self,
        envelope: &OperationEnvelopeV1,
        space_seq: u64,
        reason_code: &str,
    ) -> Result<()> {
        let encoded = rmp_serde::to_vec_named(envelope).context("encode quarantined envelope")?;
        let quarantined_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        self.connect()?.execute(
            r#"
            INSERT INTO quarantined_operations (
                space_id, client_op_id, space_seq, reason_code, envelope, quarantined_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(space_id, client_op_id) DO UPDATE SET
                space_seq = MAX(quarantined_operations.space_seq, excluded.space_seq),
                reason_code = excluded.reason_code,
                envelope = excluded.envelope,
                quarantined_at_ms = excluded.quarantined_at_ms
            "#,
            params![
                envelope.space_id.to_string(),
                envelope.client_op_id.to_string(),
                i64::try_from(space_seq).unwrap_or(i64::MAX),
                reason_code,
                encoded,
                i64::try_from(quarantined_at_ms).unwrap_or(i64::MAX),
            ],
        )?;
        Ok(())
    }

    pub(crate) fn quarantined_stream_ids(
        &self,
        space_id: Uuid,
        through_space_seq: u64,
    ) -> Result<Vec<Uuid>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            "SELECT envelope FROM quarantined_operations WHERE space_id = ?1 AND space_seq <= ?2",
        )?;
        let envelopes = statement.query_map(
            params![
                space_id.to_string(),
                i64::try_from(through_space_seq).unwrap_or(i64::MAX)
            ],
            |row| row.get::<_, Vec<u8>>(0),
        )?;
        let mut streams = std::collections::HashSet::new();
        for encoded in envelopes {
            let envelope: OperationEnvelopeV1 =
                rmp_serde::from_slice(&encoded?).context("decode quarantined envelope")?;
            streams.insert(envelope.stream_id);
        }
        let mut streams = streams.into_iter().collect::<Vec<_>>();
        streams.sort_unstable();
        Ok(streams)
    }

    pub(crate) fn quarantined_operations(
        &self,
        space_id: Uuid,
        reason_code: &str,
    ) -> Result<Vec<(OperationEnvelopeV1, u64)>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            "SELECT envelope, space_seq FROM quarantined_operations WHERE space_id = ?1 AND reason_code = ?2 ORDER BY space_seq, client_op_id",
        )?;
        let rows = statement.query_map(params![space_id.to_string(), reason_code], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.map(|row| {
            let (encoded, space_seq) = row?;
            let envelope =
                rmp_serde::from_slice(&encoded).context("decode quarantined operation envelope")?;
            Ok((envelope, u64::try_from(space_seq)?))
        })
        .collect()
    }

    pub(crate) fn remove_quarantined_operation(
        &self,
        space_id: Uuid,
        client_op_id: Uuid,
    ) -> Result<()> {
        self.connect()?.execute(
            "DELETE FROM quarantined_operations WHERE space_id = ?1 AND client_op_id = ?2",
            params![space_id.to_string(), client_op_id.to_string()],
        )?;
        Ok(())
    }

    #[cfg(test)]
    fn quarantined_reason(&self, space_id: Uuid, client_op_id: Uuid) -> Result<Option<String>> {
        self.connect()?
            .query_row(
                "SELECT reason_code FROM quarantined_operations WHERE space_id = ?1 AND client_op_id = ?2",
                params![space_id.to_string(), client_op_id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }
}

fn upsert_operation_state(conn: &Connection, state: &CachedOperationState) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO operation_states (
            client_op_id, collection_id, stream_id, logical_resource_id,
            materialized_resource_id, resource_kind, payload, deleted, space_seq,
            parent_operation_id, seed_projection_resource_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(collection_id, client_op_id) DO UPDATE SET
            stream_id = excluded.stream_id,
            logical_resource_id = excluded.logical_resource_id,
            materialized_resource_id = excluded.materialized_resource_id,
            resource_kind = excluded.resource_kind,
            payload = excluded.payload,
            deleted = excluded.deleted,
            parent_operation_id = excluded.parent_operation_id,
            seed_projection_resource_id = COALESCE(
                excluded.seed_projection_resource_id,
                operation_states.seed_projection_resource_id
            ),
            space_seq = MAX(operation_states.space_seq, excluded.space_seq)
        "#,
        params![
            state.client_op_id.to_string(),
            state.collection_id,
            state.stream_id.to_string(),
            state.logical_resource_id,
            state.materialized_resource_id,
            state.kind.route_prefix(),
            state.payload,
            i64::from(state.deleted),
            i64::try_from(state.space_seq).unwrap_or(i64::MAX),
            state.parent_operation_id.map(|value| value.to_string()),
            state.seed_projection_resource_id,
        ],
    )?;
    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, column: &str, migration_sql: &str) -> Result<()> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(());
        }
    }
    conn.execute(migration_sql, [])?;
    Ok(())
}

fn migrate_operation_state_scope(conn: &Connection) -> Result<()> {
    let mut statement = conn.prepare("PRAGMA table_info(operation_states)")?;
    let primary_key_columns = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|(_, position)| *position > 0)
        .collect::<Vec<_>>();
    if primary_key_columns.len() == 2
        && primary_key_columns
            .iter()
            .any(|(name, _)| name == "collection_id")
        && primary_key_columns
            .iter()
            .any(|(name, _)| name == "client_op_id")
    {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        BEGIN IMMEDIATE;
        ALTER TABLE operation_states RENAME TO operation_states_legacy;
        CREATE TABLE operation_states (
            collection_id TEXT NOT NULL,
            client_op_id TEXT NOT NULL,
            stream_id TEXT NOT NULL,
            logical_resource_id TEXT NOT NULL,
            materialized_resource_id TEXT NOT NULL,
            resource_kind TEXT NOT NULL,
            payload TEXT,
            deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
            space_seq INTEGER NOT NULL DEFAULT 0,
            parent_operation_id TEXT,
            seed_projection_resource_id TEXT,
            PRIMARY KEY (collection_id, client_op_id)
        );
        INSERT INTO operation_states (
            collection_id, client_op_id, stream_id, logical_resource_id,
            materialized_resource_id, resource_kind, payload, deleted, space_seq,
            parent_operation_id, seed_projection_resource_id
        )
        SELECT collection_id, client_op_id, stream_id, logical_resource_id,
               materialized_resource_id, resource_kind, payload, deleted, space_seq,
               NULL, materialized_resource_id
        FROM operation_states_legacy;
        DROP TABLE operation_states_legacy;
        CREATE INDEX idx_operation_states_resource
            ON operation_states (collection_id, logical_resource_id);
        COMMIT;
        "#,
    )?;
    Ok(())
}

fn migrate_operation_outbox_scope(conn: &Connection) -> Result<()> {
    let mut statement = conn.prepare("PRAGMA table_info(operation_outbox)")?;
    let primary_key_columns = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|(_, position)| *position > 0)
        .collect::<Vec<_>>();
    if primary_key_columns.len() == 2
        && primary_key_columns
            .iter()
            .any(|(name, _)| name == "space_id")
        && primary_key_columns
            .iter()
            .any(|(name, _)| name == "client_op_id")
    {
        return Ok(());
    }

    let legacy_rows = {
        let mut statement = conn.prepare(
            "SELECT envelope, created_at_ms FROM operation_outbox ORDER BY created_at_ms ASC",
        )?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(
        r#"
        ALTER TABLE operation_outbox RENAME TO operation_outbox_legacy;
        CREATE TABLE operation_outbox (
            space_id TEXT NOT NULL,
            client_op_id TEXT NOT NULL,
            envelope BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY (space_id, client_op_id)
        );
        "#,
    )?;
    for (encoded, created_at_ms) in legacy_rows {
        let envelope: OperationEnvelopeV1 =
            rmp_serde::from_slice(&encoded).context("decode legacy operation outbox item")?;
        transaction.execute(
            r#"
            INSERT INTO operation_outbox (space_id, client_op_id, envelope, created_at_ms)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![
                envelope.space_id.to_string(),
                envelope.client_op_id.to_string(),
                encoded,
                created_at_ms
            ],
        )?;
    }
    transaction.execute("DROP TABLE operation_outbox_legacy", [])?;
    transaction.commit()?;
    Ok(())
}

fn map_operation_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<CachedOperationState> {
    let kind: String = row.get(5)?;
    let kind = match kind.as_str() {
        "carddav" => DavResourceKind::Contact,
        "caldav" => DavResourceKind::Calendar,
        "notes" => DavResourceKind::Note,
        _ => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                format!("unsupported cached resource kind {kind:?}").into(),
            ));
        }
    };
    let client_op_id: String = row.get(0)?;
    let client_op_id = Uuid::parse_str(&client_op_id).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let stream_id: String = row.get(2)?;
    let stream_id = Uuid::parse_str(&stream_id).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(CachedOperationState {
        client_op_id,
        space_seq: u64::try_from(row.get::<_, i64>(8)?).unwrap_or(0),
        collection_id: row.get(1)?,
        stream_id,
        logical_resource_id: row.get(3)?,
        materialized_resource_id: row.get(4)?,
        kind,
        payload: row.get(6)?,
        deleted: row.get::<_, i64>(7)? != 0,
        parent_operation_id: row
            .get::<_, Option<String>>(9)?
            .map(|value| Uuid::parse_str(&value))
            .transpose()
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
        seed_projection_resource_id: row.get(10)?,
    })
}

fn record_dav_change(
    transaction: &rusqlite::Transaction<'_>,
    resource: &LocalResource,
    deleted: bool,
) -> Result<()> {
    transaction.execute(
        "INSERT INTO dav_changes (resource_kind, collection_id, resource_id, etag, deleted) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            resource.kind.route_prefix(),
            resource.collection_id,
            resource.resource_id,
            resource.etag,
            i64::from(deleted)
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        local_bridge_runner::{DavResourceKind, LocalResource, UpsertOutcome},
        operation_envelope::{EnvelopeCipherSuite, EnvelopeKind},
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        path.push(format!(
            "kamori_local_bridge_{name}_{}_{}.sqlite",
            std::process::id(),
            now
        ));
        path
    }

    fn test_envelope(space_id: Uuid, client_op_id: Uuid) -> OperationEnvelopeV1 {
        OperationEnvelopeV1 {
            space_id,
            stream_id: Uuid::new_v4(),
            client_op_id,
            author_device_id: Uuid::new_v4(),
            key_epoch: 1,
            envelope_kind: EnvelopeKind::Operation,
            cipher_suite: EnvelopeCipherSuite::Xchacha20Poly1305,
            nonce: vec![1; 24],
            ciphertext: vec![2; 16],
            signature: vec![3; 64],
        }
    }

    #[test]
    fn lww_prefers_newer_payload() {
        let db_path = temp_db_path("lww_prefers_newer_payload");
        let cache = LocalCache::new(db_path.clone(), None).expect("cache");

        let base = LocalResource {
            kind: DavResourceKind::Contact,
            collection_id: "personal".to_string(),
            resource_id: "alice.vcf".to_string(),
            payload: "VERSION:4.0".to_string(),
            etag: "etag1".to_string(),
            updated_at_ms: 1000,
        };
        let newer = LocalResource {
            payload: "VERSION:4.0\nFN:Alice".to_string(),
            etag: "etag2".to_string(),
            updated_at_ms: 2000,
            ..base.clone()
        };
        let stale = LocalResource {
            payload: "STALE".to_string(),
            etag: "etag3".to_string(),
            updated_at_ms: 1500,
            ..base.clone()
        };

        let first = cache.upsert_lww(&base).expect("insert");
        let second = cache.upsert_lww(&newer).expect("update");
        let third = cache.upsert_lww(&stale).expect("stale");

        assert_eq!(first, UpsertOutcome::Inserted);
        assert_eq!(second, UpsertOutcome::Updated);
        assert_eq!(third, UpsertOutcome::IgnoredStale);

        let saved = cache
            .get_resource(DavResourceKind::Contact, "personal", "alice.vcf")
            .expect("query")
            .expect("resource exists");
        assert_eq!(saved.payload, newer.payload);

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn rotated_credentials_recover_only_from_the_current_or_previous_token() {
        let db_path = temp_db_path("rotated_credentials");
        let cache = LocalCache::new(db_path.clone(), Some("test-encryption-key".to_string()))
            .expect("encrypted cache");
        cache
            .store_rotated_credentials("refresh-old", "access-new", "refresh-new")
            .expect("store rotation");
        for supplied in ["refresh-old", "refresh-new"] {
            assert_eq!(
                cache
                    .recover_rotated_credentials(Some(supplied))
                    .expect("recover credentials"),
                Some(("access-new".to_string(), "refresh-new".to_string()))
            );
        }
        assert_eq!(
            cache
                .recover_rotated_credentials(Some("unrelated-account-token"))
                .expect("reject unrelated token"),
            None
        );
        cache
            .clear_runtime_credentials()
            .expect("clear credentials");
        assert_eq!(
            cache
                .recover_rotated_credentials(Some("refresh-new"))
                .expect("credentials stay cleared"),
            None
        );
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn refresh_rotation_attempt_is_random_stable_and_token_scoped() {
        let db_path = temp_db_path("refresh_rotation_attempt");
        let cache = LocalCache::new(db_path.clone(), Some("test-encryption-key".to_string()))
            .expect("encrypted cache");
        let first = cache
            .begin_refresh_rotation("refresh-old")
            .expect("begin rotation");
        assert_eq!(
            cache
                .begin_refresh_rotation("refresh-old")
                .expect("resume rotation"),
            first
        );
        let second = cache
            .begin_refresh_rotation("refresh-other")
            .expect("begin other rotation");
        assert_ne!(second, first);
        cache
            .store_rotated_credentials("refresh-other", "access-new", "refresh-new")
            .expect("commit rotation");
        let third = cache
            .begin_refresh_rotation("refresh-new")
            .expect("begin next generation");
        assert_ne!(third, second);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn plaintext_cache_is_rejected_without_creating_a_backup_copy() {
        let db_path = temp_db_path("plaintext_rejected");
        LocalCache::new(db_path.clone(), None).expect("legacy plaintext cache");
        let error = match LocalCache::new(db_path.clone(), Some("test-encryption-key".to_string()))
        {
            Ok(_) => panic!("plaintext cache must fail closed"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("unencrypted legacy cache"));
        let backup = PathBuf::from(format!("{}.plain-backup", db_path.display()));
        assert!(!backup.exists());
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn authoritative_upsert_replaces_unconfirmed_wall_clock_version() {
        let db_path = temp_db_path("authoritative_upsert");
        let cache = LocalCache::new(db_path.clone(), None).expect("cache");
        let local = LocalResource {
            kind: DavResourceKind::Calendar,
            collection_id: "space".to_string(),
            resource_id: "event.ics".to_string(),
            payload: "local".to_string(),
            etag: "local-etag".to_string(),
            updated_at_ms: 1_900_000_000_000,
        };
        cache.upsert_lww(&local).expect("local upsert");
        let remote = LocalResource {
            payload: "remote".to_string(),
            etag: "remote-etag".to_string(),
            updated_at_ms: 7,
            ..local
        };
        assert_eq!(
            cache.upsert_authoritative(&remote).expect("remote upsert"),
            UpsertOutcome::Updated
        );
        assert_eq!(
            cache
                .get_resource(DavResourceKind::Calendar, "space", "event.ics")
                .expect("read")
                .expect("resource")
                .payload,
            "remote"
        );
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn sync_cursor_is_scoped_per_workspace() {
        let db_path = temp_db_path("sync_cursor_is_scoped_per_workspace");
        let cache = LocalCache::new(db_path.clone(), None).expect("cache");

        cache
            .store_last_seq("workspace:personal", 10)
            .expect("store personal");
        cache
            .store_last_seq("workspace:11111111-1111-1111-1111-111111111111", 77)
            .expect("store workspace");

        let personal = cache
            .load_last_seq("workspace:personal")
            .expect("load personal");
        let workspace = cache
            .load_last_seq("workspace:11111111-1111-1111-1111-111111111111")
            .expect("load workspace");
        let unknown = cache
            .load_last_seq("workspace:22222222-2222-2222-2222-222222222222")
            .expect("load unknown");

        assert_eq!(personal, 10);
        assert_eq!(workspace, 77);
        assert_eq!(unknown, 0);

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn advancing_sync_cursor_never_moves_it_backwards() {
        let db_path = temp_db_path("advance_sync_cursor");
        let cache = LocalCache::new(db_path.clone(), None).expect("cache");
        let scope = "space:current-state";

        cache.advance_last_seq(scope, 40).expect("advance cursor");
        cache
            .advance_last_seq(scope, 12)
            .expect("ignore older boundary");

        assert_eq!(cache.load_last_seq(scope).expect("load cursor"), 40);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn operation_acknowledgement_preserves_highest_server_sequence() {
        let db_path = temp_db_path("operation_acknowledgement");
        let cache = LocalCache::new(db_path.clone(), None).expect("cache");
        let client_op_id = Uuid::new_v4();
        cache
            .store_operation_state(&CachedOperationState {
                client_op_id,
                space_seq: 0,
                collection_id: "space".to_string(),
                stream_id: Uuid::new_v4(),
                logical_resource_id: "task.ics".to_string(),
                materialized_resource_id: "task.ics".to_string(),
                kind: DavResourceKind::Calendar,
                payload: Some("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n".to_string()),
                deleted: false,
                parent_operation_id: None,
                seed_projection_resource_id: None,
            })
            .expect("store local operation");
        cache
            .store_operation_state(&CachedOperationState {
                client_op_id,
                space_seq: 9,
                collection_id: "other-space".to_string(),
                stream_id: Uuid::new_v4(),
                logical_resource_id: "other.ics".to_string(),
                materialized_resource_id: "other.ics".to_string(),
                kind: DavResourceKind::Calendar,
                payload: Some("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n".to_string()),
                deleted: false,
                parent_operation_id: None,
                seed_projection_resource_id: None,
            })
            .expect("store same operation id in another space");

        cache
            .acknowledge_operation("space", client_op_id, 42)
            .expect("acknowledge operation");
        cache
            .acknowledge_operation("space", client_op_id, 7)
            .expect("ignore stale acknowledgement");

        let stored = cache
            .load_operation_state("space", client_op_id)
            .expect("load operation")
            .expect("operation exists");
        assert_eq!(stored.space_seq, 42);
        assert_eq!(
            cache
                .load_operation_state("other-space", client_op_id)
                .expect("load other operation")
                .expect("other operation exists")
                .space_seq,
            9
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn operation_state_and_resource_head_are_committed_together() {
        let db_path = temp_db_path("operation_state_and_head");
        let cache = LocalCache::new(db_path.clone(), None).expect("cache");
        let client_op_id = Uuid::new_v4();
        let state = CachedOperationState {
            client_op_id,
            space_seq: 3,
            collection_id: "space".to_string(),
            stream_id: Uuid::new_v4(),
            logical_resource_id: "task.ics".to_string(),
            materialized_resource_id: "task.ics".to_string(),
            kind: DavResourceKind::Calendar,
            payload: Some("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n".to_string()),
            deleted: false,
            parent_operation_id: None,
            seed_projection_resource_id: None,
        };

        cache
            .store_operation_state_and_head(&state, "task.ics")
            .expect("commit state and head");

        assert_eq!(
            cache
                .load_operation_state("space", client_op_id)
                .expect("load operation")
                .expect("operation exists")
                .space_seq,
            3
        );
        assert_eq!(
            cache
                .load_resource_head("space", "task.ics")
                .expect("load resource head"),
            Some(client_op_id)
        );
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn legacy_operation_state_key_is_migrated_to_space_scope() {
        let db_path = temp_db_path("operation_scope_migration");
        let operation_id = Uuid::new_v4();
        let stream_id = Uuid::new_v4();
        let conn = Connection::open(&db_path).expect("open legacy cache");
        conn.execute_batch(
            r#"
            CREATE TABLE operation_states (
                client_op_id TEXT PRIMARY KEY,
                collection_id TEXT NOT NULL,
                stream_id TEXT NOT NULL,
                logical_resource_id TEXT NOT NULL,
                materialized_resource_id TEXT NOT NULL,
                resource_kind TEXT NOT NULL,
                payload TEXT,
                deleted INTEGER NOT NULL,
                space_seq INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )
        .expect("create legacy operation table");
        conn.execute(
            r#"
            INSERT INTO operation_states (
                client_op_id, collection_id, stream_id, logical_resource_id,
                materialized_resource_id, resource_kind, payload, deleted, space_seq
            ) VALUES (?1, 'space', ?2, 'task.ics', 'task.ics', 'caldav', NULL, 1, 12)
            "#,
            params![operation_id.to_string(), stream_id.to_string()],
        )
        .expect("insert legacy operation");
        drop(conn);

        let cache = LocalCache::new(db_path.clone(), None).expect("migrate cache");
        let migrated = cache
            .load_operation_state("space", operation_id)
            .expect("load migrated operation")
            .expect("migrated operation exists");
        assert_eq!(migrated.space_seq, 12);

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn legacy_outbox_is_preserved_and_scoped_per_space() {
        let db_path = temp_db_path("outbox_scope_migration");
        let client_op_id = Uuid::new_v4();
        let first = test_envelope(Uuid::new_v4(), client_op_id);
        let second = test_envelope(Uuid::new_v4(), client_op_id);
        let encoded = rmp_serde::to_vec_named(&first).expect("encode legacy envelope");
        let conn = Connection::open(&db_path).expect("open legacy cache");
        conn.execute_batch(
            r#"
            CREATE TABLE operation_outbox (
                client_op_id TEXT PRIMARY KEY,
                envelope BLOB NOT NULL,
                created_at_ms INTEGER NOT NULL
            );
            "#,
        )
        .expect("create legacy outbox");
        conn.execute(
            "INSERT INTO operation_outbox (client_op_id, envelope, created_at_ms) VALUES (?1, ?2, 1)",
            params![client_op_id.to_string(), encoded],
        )
        .expect("insert legacy envelope");
        drop(conn);

        let cache = LocalCache::new(db_path.clone(), None).expect("migrate cache");
        cache
            .queue_operation(&second, 2)
            .expect("queue same id in another space");
        let queued = cache.list_queued_operations().expect("list scoped outbox");
        assert_eq!(queued.len(), 2);
        assert!(queued.iter().any(|item| item.space_id == first.space_id));
        assert!(queued.iter().any(|item| item.space_id == second.space_id));

        cache
            .remove_queued_operation(first.space_id, client_op_id)
            .expect("remove only first space item");
        let remaining = cache
            .list_queued_operations()
            .expect("list remaining outbox");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].space_id, second.space_id);

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn outbox_preserves_insertion_order_when_timestamps_match() {
        let db_path = temp_db_path("outbox_insertion_order");
        let cache = LocalCache::new(db_path.clone(), None).expect("cache");
        let space_id = Uuid::new_v4();
        let first = test_envelope(space_id, Uuid::new_v4());
        let second = test_envelope(space_id, Uuid::new_v4());

        cache.queue_operation(&first, 7).expect("queue parent");
        cache.queue_operation(&second, 7).expect("queue child");

        let queued = cache.list_queued_operations().expect("list outbox");
        assert_eq!(queued.len(), 2);
        assert_eq!(queued[0].client_op_id, first.client_op_id);
        assert_eq!(queued[1].client_op_id, second.client_op_id);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn quarantined_envelope_is_deduplicated_per_space_and_operation() {
        let db_path = temp_db_path("quarantined_envelope");
        let cache = LocalCache::new(db_path.clone(), None).expect("cache");
        let space_id = Uuid::new_v4();
        let client_op_id = Uuid::new_v4();
        let envelope = test_envelope(space_id, client_op_id);

        cache
            .quarantine_operation(&envelope, 4, "invalid_ciphertext")
            .expect("quarantine first");
        cache
            .quarantine_operation(&envelope, 7, "invalid_operation")
            .expect("update quarantine");

        assert_eq!(
            cache
                .quarantined_reason(space_id, client_op_id)
                .expect("read quarantine")
                .as_deref(),
            Some("invalid_operation")
        );
        assert!(
            cache
                .quarantined_operations(space_id, "unresolved_pim_graph")
                .expect("filter quarantine")
                .is_empty()
        );
        let pending = cache
            .quarantined_operations(space_id, "invalid_operation")
            .expect("list quarantine");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0.client_op_id, client_op_id);
        assert_eq!(pending[0].1, 7);
        cache
            .remove_quarantined_operation(space_id, client_op_id)
            .expect("remove quarantine");
        assert!(
            cache
                .quarantined_operations(space_id, "invalid_operation")
                .expect("list removed quarantine")
                .is_empty()
        );
        let _ = std::fs::remove_file(db_path);
    }

    #[cfg(feature = "local-bridge")]
    #[test]
    fn graph_reconciliation_does_not_emit_noop_dav_changes() {
        let db_path = temp_db_path("graph_reconciliation_changes");
        let cache = LocalCache::new(db_path.clone(), None).expect("cache");
        let stream_id = Uuid::new_v4();
        let operation_id = Uuid::new_v4();
        let payload =
            format!("BEGIN:VCARD\r\nVERSION:4.0\r\nUID:{stream_id}\r\nFN:Alice\r\nEND:VCARD\r\n");
        cache
            .store_operation_state(&CachedOperationState {
                client_op_id: operation_id,
                space_seq: 1,
                collection_id: "space".to_string(),
                stream_id,
                logical_resource_id: format!("{stream_id}.vcf"),
                materialized_resource_id: format!("{stream_id}.vcf"),
                kind: DavResourceKind::Contact,
                payload: Some(payload),
                deleted: false,
                parent_operation_id: None,
                seed_projection_resource_id: None,
            })
            .expect("store state");
        cache
            .reconcile_pim_stream(
                "space",
                stream_id,
                &format!("{stream_id}.vcf"),
                DavResourceKind::Contact,
            )
            .expect("first reconcile");
        let first_revision = cache
            .latest_dav_revision(DavResourceKind::Contact, "space")
            .expect("first revision");
        cache
            .reconcile_pim_stream(
                "space",
                stream_id,
                &format!("{stream_id}.vcf"),
                DavResourceKind::Contact,
            )
            .expect("second reconcile");
        assert_eq!(
            cache
                .latest_dav_revision(DavResourceKind::Contact, "space")
                .expect("second revision"),
            first_revision
        );
        let _ = std::fs::remove_file(db_path);
    }

    #[cfg(feature = "local-bridge")]
    #[test]
    fn dav_change_journal_records_updates_and_tombstones_atomically() {
        let db_path = temp_db_path("dav_change_journal");
        let cache = LocalCache::new(db_path.clone(), None).expect("cache");
        let original = LocalResource {
            kind: DavResourceKind::Calendar,
            collection_id: "space".to_string(),
            resource_id: "meeting.ics".to_string(),
            payload: "BEGIN:VCALENDAR\nEND:VCALENDAR".to_string(),
            etag: "one".to_string(),
            updated_at_ms: 1,
        };
        let updated = LocalResource {
            etag: "two".to_string(),
            updated_at_ms: 2,
            ..original.clone()
        };

        cache.upsert_lww(&original).expect("insert");
        let first_revision = cache
            .latest_dav_revision(DavResourceKind::Calendar, "space")
            .expect("first revision");
        cache.upsert_lww(&updated).expect("update");
        cache
            .delete_resource(DavResourceKind::Calendar, "space", "meeting.ics")
            .expect("delete");

        let changes = cache
            .list_dav_changes_since(DavResourceKind::Calendar, "space", first_revision)
            .expect("changes");
        assert_eq!(changes.len(), 2);
        assert!(!changes[0].deleted);
        assert!(changes[1].deleted);
        assert!(changes[1].revision > changes[0].revision);

        let _ = std::fs::remove_file(db_path);
    }
}
