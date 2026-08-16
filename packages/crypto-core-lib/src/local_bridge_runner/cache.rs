#[cfg(feature = "local-bridge")]
use super::types::DavChange;
use super::{DavResourceKind, LocalResource, UpsertOutcome};
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;
use tracing::warn;
use uuid::Uuid;

use crate::operation_envelope::OperationEnvelopeV1;

const DEFAULT_SYNC_SCOPE: &str = "workspace:personal";

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
            let can_migrate_plaintext = cache.db_key.is_some()
                && cache.db_path.exists()
                && cache.is_sqlcipher_mismatch_error(&error)
                && cache.can_open_as_plaintext_sqlite();

            if can_migrate_plaintext {
                let backup_path = cache.backup_legacy_plaintext_db()?;
                warn!(
                    db_path = %cache.db_path.display(),
                    backup_path = %backup_path.display(),
                    "legacy unencrypted cache detected; moved to backup and initialized encrypted cache",
                );
                cache.init_schema()?;
            } else {
                return Err(error);
            }
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

    /// Moves old plaintext cache aside before creating a fresh encrypted cache.
    fn backup_legacy_plaintext_db(&self) -> Result<PathBuf> {
        let base = self.db_path.to_string_lossy();
        let mut index = 0_u32;
        let mut backup = PathBuf::from(format!("{base}.plain-backup"));

        while backup.exists() {
            index = index.saturating_add(1);
            backup = PathBuf::from(format!("{base}.plain-backup-{index}"));
        }

        std::fs::rename(&self.db_path, &backup).with_context(|| {
            format!(
                "move legacy plaintext db {} to {}",
                self.db_path.display(),
                backup.display()
            )
        })?;

        Ok(backup)
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

            CREATE TABLE IF NOT EXISTS operation_outbox (
                client_op_id TEXT PRIMARY KEY,
                envelope BLOB NOT NULL,
                created_at_ms INTEGER NOT NULL
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

            INSERT OR IGNORE INTO sync_state (id, last_seq_id) VALUES (1, 0);
            INSERT INTO sync_cursors (scope, last_seq_id)
            SELECT 'workspace:personal', last_seq_id
            FROM sync_state
            WHERE id = 1
            ON CONFLICT(scope) DO NOTHING;
            "#,
        )?;
        Ok(())
    }

    /// Inserts/updates a resource using LWW timestamp semantics.
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

    pub(crate) fn store_resource_head(
        &self,
        collection_id: &str,
        resource_id: &str,
        client_op_id: Uuid,
    ) -> Result<()> {
        self.connect()?.execute(
            r#"
            INSERT INTO resource_heads (collection_id, resource_id, client_op_id)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(collection_id, resource_id) DO UPDATE SET
                client_op_id = excluded.client_op_id
            "#,
            params![collection_id, resource_id, client_op_id.to_string()],
        )?;
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
        self.connect()?.execute(
            "INSERT OR REPLACE INTO operation_outbox (client_op_id, envelope, created_at_ms) VALUES (?1, ?2, ?3)",
            params![envelope.client_op_id.to_string(), encoded, created_at_ms],
        )?;
        Ok(())
    }

    pub(crate) fn list_queued_operations(&self) -> Result<Vec<OperationEnvelopeV1>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            "SELECT envelope FROM operation_outbox ORDER BY created_at_ms ASC, client_op_id ASC",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
        let mut operations = Vec::new();
        for row in rows {
            operations.push(rmp_serde::from_slice(&row?).context("decode operation outbox item")?);
        }
        Ok(operations)
    }

    pub(crate) fn remove_queued_operation(&self, client_op_id: Uuid) -> Result<()> {
        self.connect()?.execute(
            "DELETE FROM operation_outbox WHERE client_op_id = ?1",
            params![client_op_id.to_string()],
        )?;
        Ok(())
    }
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
    use crate::local_bridge_runner::{DavResourceKind, LocalResource, UpsertOutcome};
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
