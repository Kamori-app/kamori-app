//! Encrypted local sync runtime with an optional desktop-only DAV projection.

mod cache;
mod cloud;
#[cfg(feature = "local-bridge")]
mod dav;
mod types;

pub use types::{
    DavResourceKind, LocalBridgeConfig, LocalDeviceIdentity, LocalResource, UpsertOutcome,
};

use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
#[cfg(feature = "local-bridge")]
use axum::{Router, routing::any};
use cache::{CachedOperationState, LocalCache};
use cloud::CloudSyncClient;
#[cfg(feature = "local-bridge")]
use dav::dav_dispatch;
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
#[cfg(feature = "local-bridge")]
use tokio::sync::Mutex;
use tokio::sync::RwLock;
#[cfg(feature = "local-bridge")]
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};
use tracing::warn;
#[cfg(feature = "local-bridge")]
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    operation_envelope::{EnvelopeKind, OperationEnvelopeV1, OperationSealContext},
    pim::{PimDeleteV1, PimOperationV1, PimResourceKind, PimSnapshotV1, PimUpsertV1, PimValue},
};

#[derive(Clone)]
pub struct LocalBridgeRunner {
    state: Arc<LocalBridgeState>,
    #[cfg(feature = "local-bridge")]
    bind_addr: std::net::SocketAddr,
    #[cfg(feature = "local-bridge")]
    lifecycle: Arc<Mutex<ServerLifecycle>>,
}

impl LocalBridgeRunner {
    pub fn new(config: LocalBridgeConfig) -> Result<Self> {
        if config.cloud_base_url.trim().is_empty() || config.access_token.trim().is_empty() {
            return Err(anyhow!("cloud URL and access token are required"));
        }
        let LocalBridgeConfig {
            sqlite_path,
            sqlite_key,
            bind_addr,
            cloud_base_url,
            access_token,
            refresh_token,
            device_identity,
            dav_username,
            dav_password,
        } = config;
        #[cfg(not(feature = "local-bridge"))]
        let _ = (bind_addr, dav_username, dav_password);
        Ok(Self {
            state: Arc::new(LocalBridgeState {
                cache: LocalCache::new(sqlite_path, sqlite_key)?,
                collection_keys: RwLock::new(HashMap::new()),
                cloud: CloudSyncClient::new(cloud_base_url, access_token, refresh_token)?,
                device_identity,
                #[cfg(feature = "local-bridge")]
                dav_credentials: dav_username.zip(dav_password),
            }),
            #[cfg(feature = "local-bridge")]
            bind_addr,
            #[cfg(feature = "local-bridge")]
            lifecycle: Arc::new(Mutex::new(ServerLifecycle::default())),
        })
    }

    #[cfg(feature = "local-bridge")]
    pub fn bind_addr(&self) -> std::net::SocketAddr {
        self.bind_addr
    }

    #[cfg(feature = "local-bridge")]
    pub async fn start(&self) -> Result<()> {
        let mut lifecycle = self.lifecycle.lock().await;
        if lifecycle.handle.is_some() {
            return Ok(());
        }
        if self.state.dav_credentials.is_none() {
            return Err(anyhow!("dedicated DAV credentials are required"));
        }
        if !self.bind_addr.ip().is_loopback() {
            return Err(anyhow!(
                "the local DAV bridge may only bind to a loopback address"
            ));
        }
        let listener = TcpListener::bind(self.bind_addr)
            .await
            .with_context(|| format!("bind local bridge to {}", self.bind_addr))?;
        let addr = listener.local_addr().context("read local listener addr")?;
        let app = Router::new()
            .route("/", any(dav_dispatch))
            .route("/{*path}", any(dav_dispatch))
            .with_state(self.state.clone());
        let (tx, rx) = oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            info!(%addr, "local DAV bridge started");
            if let Err(error) = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await
            {
                error!(?error, "local DAV bridge exited with an error");
            }
        });
        lifecycle.shutdown_tx = Some(tx);
        lifecycle.handle = Some(handle);
        Ok(())
    }

    #[cfg(feature = "local-bridge")]
    pub async fn stop(&self) -> Result<()> {
        let mut lifecycle = self.lifecycle.lock().await;
        if let Some(tx) = lifecycle.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = lifecycle.handle.take() {
            let _ = handle.await;
        }
        Ok(())
    }

    #[cfg(feature = "local-bridge")]
    pub async fn is_running(&self) -> bool {
        self.lifecycle.lock().await.handle.is_some()
    }

    pub async fn register_collection_key(&self, collection_id: impl Into<String>, key: [u8; 32]) {
        self.register_collection_key_epoch(collection_id, 1, key)
            .await;
    }

    pub async fn register_collection_key_epoch(
        &self,
        collection_id: impl Into<String>,
        key_epoch: u32,
        key: [u8; 32],
    ) {
        let collection_id = collection_id.into();
        match Uuid::parse_str(&collection_id) {
            Ok(space_id) if key_epoch > 0 => {
                self.state
                    .collection_keys
                    .write()
                    .await
                    .entry(space_id)
                    .or_default()
                    .insert(key_epoch, key);
            }
            _ => warn!(%collection_id, "ignored invalid security-space key registration"),
        }
    }

    pub async fn unregister_collection_key(&self, collection_id: impl AsRef<str>) -> Result<()> {
        let id = Uuid::parse_str(collection_id.as_ref()).context("invalid security-space id")?;
        self.state
            .collection_keys
            .write()
            .await
            .remove(&id)
            .ok_or_else(|| anyhow!("missing key for security space {id}"))?;
        Ok(())
    }

    /// Flushes durable local writes and pulls all accessible operations for registered spaces.
    pub async fn sync_once(&self) -> Result<u64> {
        self.state.flush_outbox().await?;
        let spaces = self.state.collection_keys.read().await.clone();
        let mut applied = 0_u64;
        for (space_id, keys) in spaces {
            applied = applied.saturating_add(self.state.sync_space(space_id, &keys).await?);
        }
        Ok(applied)
    }

    pub async fn current_refresh_token(&self) -> Option<String> {
        self.state.cloud.current_refresh_token().await
    }

    /// Lists decrypted cached resources for a first-party client projection.
    pub async fn list_cached_resources(
        &self,
        space_id: Uuid,
        kind: DavResourceKind,
    ) -> Result<Vec<LocalResource>> {
        self.state.list_resources(kind, space_id.to_string()).await
    }

    /// Writes a field-level first-party PIM operation and retains it in the
    /// encrypted outbox when the cloud is temporarily unavailable.
    pub async fn upsert_pim_item(
        &self,
        space_id: Uuid,
        resource_id: Uuid,
        resource_kind: PimResourceKind,
        fields: BTreeMap<String, PimValue>,
    ) -> Result<()> {
        self.state
            .upsert_pim_item_and_push(space_id, resource_id, resource_kind, fields)
            .await
    }

    /// Tombstones a first-party PIM item and queues the signed operation.
    pub async fn delete_pim_item(
        &self,
        space_id: Uuid,
        resource_id: Uuid,
        resource_kind: PimResourceKind,
    ) -> Result<()> {
        self.state
            .delete_pim_item_and_push(space_id, resource_id, resource_kind)
            .await
    }

    /// Creates one signed encrypted current-state snapshot per active stream.
    pub async fn build_rotation_snapshots(
        &self,
        space_id: Uuid,
        new_key_epoch: u32,
        new_space_key: [u8; 32],
        covers_through_space_seq: u64,
    ) -> Result<Vec<OperationEnvelopeV1>> {
        anyhow::ensure!(new_key_epoch > 0, "new key epoch must be positive");
        let identity = self
            .state
            .device_identity
            .as_ref()
            .ok_or_else(|| anyhow!("approved device identity is required for snapshots"))?;
        anyhow::ensure!(
            self.state
                .collection_keys
                .read()
                .await
                .contains_key(&space_id),
            "security-space key is not registered"
        );
        self.state
            .build_rotation_snapshots(
                space_id,
                new_key_epoch,
                new_space_key,
                covers_through_space_seq,
                identity,
            )
            .await
    }
}

#[cfg(feature = "local-bridge")]
#[derive(Default)]
struct ServerLifecycle {
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

#[cfg(feature = "local-bridge")]
#[derive(Debug)]
pub(crate) struct PutResult {
    pub(crate) outcome: UpsertOutcome,
    pub(crate) etag: String,
    pub(crate) cloud_space_seq: Option<u64>,
    pub(crate) cloud_pushed: bool,
}

pub(crate) struct LocalBridgeState {
    cache: LocalCache,
    collection_keys: RwLock<HashMap<Uuid, BTreeMap<u32, [u8; 32]>>>,
    cloud: CloudSyncClient,
    device_identity: Option<LocalDeviceIdentity>,
    #[cfg(feature = "local-bridge")]
    dav_credentials: Option<(String, String)>,
}

impl LocalBridgeState {
    async fn sync_space(&self, space_id: Uuid, keys: &BTreeMap<u32, [u8; 32]>) -> Result<u64> {
        let directory = self
            .cloud
            .fetch_space_devices(space_id)
            .await?
            .into_iter()
            .map(|device| (device.device_id, device.signing_public_key))
            .collect::<HashMap<_, _>>();
        let scope = format!("space:{space_id}");
        let mut cursor = self.load_last_seq(&scope).await?;
        let mut applied = 0_u64;
        loop {
            let page = self.cloud.fetch_operations(space_id, cursor).await?;
            let operation_count = page.operations.len();
            for stored in &page.operations {
                let public_key = directory
                    .get(&stored.envelope.author_device_id)
                    .ok_or_else(|| anyhow!("operation author key is missing"))?;
                stored
                    .envelope
                    .verify(public_key)
                    .context("verify operation author signature")?;
                if let Some(key) = keys.get(&stored.envelope.key_epoch) {
                    let plaintext = stored.envelope.open(key)?;
                    match stored.envelope.envelope_kind {
                        EnvelopeKind::Operation => {
                            let operation = PimOperationV1::decode(&plaintext)?;
                            self.apply_pim_operation(
                                space_id,
                                stored.space_seq,
                                &stored.envelope,
                                operation,
                            )
                            .await?;
                            applied = applied.saturating_add(1);
                        }
                        EnvelopeKind::Snapshot => {
                            let snapshot = PimSnapshotV1::decode(&plaintext)?;
                            self.apply_pim_snapshot(
                                space_id,
                                stored.space_seq,
                                &stored.envelope,
                                snapshot,
                            )
                            .await?;
                            applied = applied.saturating_add(1);
                        }
                        EnvelopeKind::Control => {
                            return Err(anyhow!(
                                "unsupported mandatory key-control envelope version"
                            ));
                        }
                    }
                } else {
                    warn!(%space_id, key_epoch = stored.envelope.key_epoch, "operation belongs to an unavailable historical key epoch");
                }
            }
            if operation_count == 0 || page.next_cursor <= cursor {
                break;
            }
            cursor = page.next_cursor;
            self.store_last_seq(&scope, cursor).await?;
            if operation_count < 1000 {
                break;
            }
        }
        Ok(applied)
    }

    async fn apply_pim_operation(
        &self,
        space_id: Uuid,
        space_seq: u64,
        envelope: &OperationEnvelopeV1,
        operation: PimOperationV1,
    ) -> Result<()> {
        let collection_id = space_id.to_string();
        let (resource_kind, resource_id, dependencies) = match &operation {
            PimOperationV1::Upsert(value) => (
                value.resource_kind,
                dav_resource_id(value),
                value.dependencies.as_slice(),
            ),
            PimOperationV1::Delete(value) => (
                value.resource_kind,
                value
                    .projection_resource_id
                    .clone()
                    .unwrap_or_else(|| default_resource_id(value.resource_kind, value.resource_id)),
                value.dependencies.as_slice(),
            ),
        };
        let dav_kind = dav_kind(resource_kind);
        if self
            .load_operation_state(envelope.client_op_id)
            .await?
            .is_some()
        {
            return Ok(());
        }
        let current_head = self
            .load_resource_head(&collection_id, &resource_id)
            .await?;
        let mut dependency_states = Vec::new();
        for dependency in dependencies {
            if let Some(state) = self.load_operation_state(*dependency).await? {
                dependency_states.push(state);
            }
        }
        dependency_states.sort_by_key(|state| state.client_op_id);
        let base_state = current_head
            .and_then(|head| {
                dependency_states
                    .iter()
                    .find(|state| state.client_op_id == head)
            })
            .or_else(|| dependency_states.last());
        let conflict = current_head.is_some_and(|head| !dependencies.contains(&head));
        if matches!(operation, PimOperationV1::Delete(_)) {
            let target_resource_id = if conflict {
                base_state
                    .filter(|state| state.materialized_resource_id != resource_id)
                    .map(|state| state.materialized_resource_id.clone())
                    .unwrap_or_else(|| format!("{resource_id}.conflict-{}", envelope.client_op_id))
            } else {
                resource_id.clone()
            };
            if !conflict || target_resource_id != resource_id {
                self.delete_resource(dav_kind, collection_id.clone(), target_resource_id.clone())
                    .await?;
            }
            self.store_operation_state(CachedOperationState {
                client_op_id: envelope.client_op_id,
                collection_id: collection_id.clone(),
                stream_id: envelope.stream_id,
                logical_resource_id: resource_id.clone(),
                materialized_resource_id: target_resource_id,
                kind: dav_kind,
                payload: None,
                deleted: true,
            })
            .await?;
            if !conflict {
                self.store_resource_head(collection_id, resource_id, envelope.client_op_id)
                    .await?;
            }
            return Ok(());
        }
        let PimOperationV1::Upsert(upsert) = operation else {
            unreachable!();
        };
        let existing_payload = base_state
            .and_then(|state| state.payload.as_deref())
            .map(str::to_owned);
        let payload = materialize_projection(&upsert, existing_payload.as_deref());
        let target_resource_id = if conflict {
            base_state
                .filter(|state| state.materialized_resource_id != resource_id)
                .map(|state| state.materialized_resource_id.clone())
                .unwrap_or_else(|| format!("{resource_id}.conflict-{}", envelope.client_op_id))
        } else {
            resource_id.clone()
        };
        let resource = LocalResource {
            kind: dav_kind,
            collection_id: collection_id.clone(),
            resource_id: target_resource_id.clone(),
            etag: compute_etag(payload.as_bytes()),
            payload: payload.clone(),
            updated_at_ms: i64::try_from(space_seq).unwrap_or(i64::MAX),
        };
        self.upsert_authoritative(resource).await?;
        self.store_operation_state(CachedOperationState {
            client_op_id: envelope.client_op_id,
            collection_id: collection_id.clone(),
            stream_id: envelope.stream_id,
            logical_resource_id: resource_id.clone(),
            materialized_resource_id: target_resource_id,
            kind: dav_kind,
            payload: Some(payload),
            deleted: false,
        })
        .await?;
        if !conflict {
            self.store_resource_head(collection_id, resource_id, envelope.client_op_id)
                .await?;
        }
        Ok(())
    }

    async fn apply_pim_snapshot(
        &self,
        space_id: Uuid,
        space_seq: u64,
        envelope: &OperationEnvelopeV1,
        snapshot: PimSnapshotV1,
    ) -> Result<()> {
        anyhow::ensure!(
            snapshot.resource_id == envelope.stream_id,
            "snapshot stream mismatch"
        );
        anyhow::ensure!(
            snapshot.covers_through_space_seq <= space_seq,
            "snapshot coverage exceeds its transport position"
        );
        let collection_id = space_id.to_string();
        let kind = dav_kind(snapshot.resource_kind);
        let payload = if snapshot.deleted {
            self.delete_resource(
                kind,
                collection_id.clone(),
                snapshot.projection_resource_id.clone(),
            )
            .await?;
            None
        } else {
            let projection = String::from_utf8(snapshot.materialized_projection)
                .context("snapshot projection is not UTF-8")?;
            self.upsert_authoritative(LocalResource {
                kind,
                collection_id: collection_id.clone(),
                resource_id: snapshot.projection_resource_id.clone(),
                etag: compute_etag(projection.as_bytes()),
                payload: projection.clone(),
                updated_at_ms: i64::try_from(space_seq).unwrap_or(i64::MAX),
            })
            .await?;
            Some(projection)
        };
        self.store_operation_state(CachedOperationState {
            client_op_id: snapshot.head_operation_id,
            collection_id: collection_id.clone(),
            stream_id: envelope.stream_id,
            logical_resource_id: snapshot.projection_resource_id.clone(),
            materialized_resource_id: snapshot.projection_resource_id.clone(),
            kind,
            payload,
            deleted: snapshot.deleted,
        })
        .await?;
        self.store_resource_head(
            collection_id,
            snapshot.projection_resource_id,
            snapshot.head_operation_id,
        )
        .await
    }

    pub(crate) async fn list_resources(
        &self,
        kind: DavResourceKind,
        collection_id: String,
    ) -> Result<Vec<LocalResource>> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.list_resources(kind, &collection_id)).await?
    }

    #[cfg(feature = "local-bridge")]
    pub(crate) async fn registered_collection_ids(&self) -> Vec<String> {
        self.collection_keys
            .read()
            .await
            .keys()
            .map(Uuid::to_string)
            .collect()
    }

    #[cfg(feature = "local-bridge")]
    pub(crate) async fn latest_dav_revision(
        &self,
        kind: DavResourceKind,
        collection_id: String,
    ) -> Result<u64> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.latest_dav_revision(kind, &collection_id)).await?
    }

    #[cfg(feature = "local-bridge")]
    pub(crate) async fn list_dav_changes_since(
        &self,
        kind: DavResourceKind,
        collection_id: String,
        revision: u64,
    ) -> Result<Vec<types::DavChange>> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || {
            cache.list_dav_changes_since(kind, &collection_id, revision)
        })
        .await?
    }

    pub(crate) async fn get_resource(
        &self,
        kind: DavResourceKind,
        collection_id: String,
        resource_id: String,
    ) -> Result<Option<LocalResource>> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.get_resource(kind, &collection_id, &resource_id))
            .await?
    }

    async fn upsert(&self, resource: LocalResource) -> Result<(UpsertOutcome, LocalResource)> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || {
            let outcome = cache.upsert_lww(&resource)?;
            Ok((outcome, resource))
        })
        .await?
    }

    async fn delete_resource(
        &self,
        kind: DavResourceKind,
        collection_id: String,
        resource_id: String,
    ) -> Result<bool> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || {
            cache.delete_resource(kind, &collection_id, &resource_id)
        })
        .await?
    }

    async fn load_last_seq(&self, scope: &str) -> Result<u64> {
        let cache = self.cache.clone();
        let scope = scope.to_string();
        tokio::task::spawn_blocking(move || cache.load_last_seq(&scope)).await?
    }

    async fn store_last_seq(&self, scope: &str, seq: u64) -> Result<()> {
        let cache = self.cache.clone();
        let scope = scope.to_string();
        tokio::task::spawn_blocking(move || cache.store_last_seq(&scope, seq)).await?
    }

    async fn load_resource_head(
        &self,
        collection_id: &str,
        resource_id: &str,
    ) -> Result<Option<Uuid>> {
        let cache = self.cache.clone();
        let collection_id = collection_id.to_string();
        let resource_id = resource_id.to_string();
        tokio::task::spawn_blocking(move || cache.load_resource_head(&collection_id, &resource_id))
            .await?
    }

    async fn store_resource_head(
        &self,
        collection_id: String,
        resource_id: String,
        operation_id: Uuid,
    ) -> Result<()> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || {
            cache.store_resource_head(&collection_id, &resource_id, operation_id)
        })
        .await?
    }

    async fn load_operation_state(
        &self,
        operation_id: Uuid,
    ) -> Result<Option<CachedOperationState>> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.load_operation_state(operation_id)).await?
    }

    async fn store_operation_state(&self, state: CachedOperationState) -> Result<()> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.store_operation_state(&state)).await?
    }

    async fn build_rotation_snapshots(
        &self,
        space_id: Uuid,
        new_key_epoch: u32,
        new_space_key: [u8; 32],
        covers_through_space_seq: u64,
        identity: &LocalDeviceIdentity,
    ) -> Result<Vec<OperationEnvelopeV1>> {
        let cache = self.cache.clone();
        let collection_id = space_id.to_string();
        let states =
            tokio::task::spawn_blocking(move || cache.list_head_states(&collection_id)).await??;
        let signing_key = SigningKey::from_bytes(&identity.signing_private_key);
        states
            .into_iter()
            .map(|state| {
                let snapshot = PimSnapshotV1 {
                    schema_version: PimSnapshotV1::SCHEMA_VERSION,
                    covers_through_space_seq,
                    resource_kind: pim_kind_from_cached_state(&state),
                    resource_id: state.stream_id,
                    projection_resource_id: state.logical_resource_id,
                    head_operation_id: state.client_op_id,
                    deleted: state.deleted,
                    materialized_projection: state.payload.unwrap_or_default().into_bytes(),
                };
                OperationEnvelopeV1::seal_xchacha(
                    OperationSealContext {
                        space_id,
                        stream_id: state.stream_id,
                        client_op_id: Uuid::new_v4(),
                        author_device_id: identity.device_id,
                        key_epoch: new_key_epoch,
                        envelope_kind: EnvelopeKind::Snapshot,
                    },
                    &snapshot.encode()?,
                    &new_space_key,
                    &signing_key,
                )
            })
            .collect()
    }

    async fn upsert_pim_item_and_push(
        &self,
        space_id: Uuid,
        resource_id: Uuid,
        resource_kind: PimResourceKind,
        fields: BTreeMap<String, PimValue>,
    ) -> Result<()> {
        let (key_epoch, space_key) = self.current_space_key(space_id).await?;
        let identity = self
            .device_identity
            .as_ref()
            .ok_or_else(|| anyhow!("this device has not been approved for encrypted writes"))?;
        let projection_resource_id = default_resource_id(resource_kind, resource_id);
        let dependencies = self
            .load_resource_head(&space_id.to_string(), &projection_resource_id)
            .await?
            .into_iter()
            .collect();
        let upsert = PimUpsertV1 {
            resource_kind,
            resource_id,
            dependencies,
            fields,
            raw_projection: Vec::new(),
        };
        let existing = self
            .get_resource(
                dav_kind(resource_kind),
                space_id.to_string(),
                projection_resource_id.clone(),
            )
            .await?;
        let payload = materialize_projection(
            &upsert,
            existing.as_ref().map(|resource| resource.payload.as_str()),
        );
        let operation = PimOperationV1::Upsert(upsert);
        let client_op_id = Uuid::new_v4();
        let envelope = OperationEnvelopeV1::seal_xchacha(
            OperationSealContext {
                space_id,
                stream_id: resource_id,
                client_op_id,
                author_device_id: identity.device_id,
                key_epoch,
                envelope_kind: EnvelopeKind::Operation,
            },
            &operation.encode()?,
            &space_key,
            &SigningKey::from_bytes(&identity.signing_private_key),
        )?;
        let timestamp = now_unix_ms();
        self.queue_operation(envelope.clone(), timestamp).await?;
        let materialized_payload = payload.clone();
        self.upsert(LocalResource {
            kind: dav_kind(resource_kind),
            collection_id: space_id.to_string(),
            resource_id: projection_resource_id.clone(),
            etag: compute_etag(payload.as_bytes()),
            payload,
            updated_at_ms: timestamp,
        })
        .await?;
        self.store_operation_state(CachedOperationState {
            client_op_id,
            collection_id: space_id.to_string(),
            stream_id: resource_id,
            logical_resource_id: projection_resource_id.clone(),
            materialized_resource_id: projection_resource_id.clone(),
            kind: dav_kind(resource_kind),
            payload: Some(materialized_payload),
            deleted: false,
        })
        .await?;
        self.store_resource_head(space_id.to_string(), projection_resource_id, client_op_id)
            .await?;
        if self.cloud.append_operation(&envelope).await.is_ok() {
            self.remove_queued_operation(client_op_id).await?;
        }
        Ok(())
    }

    async fn delete_pim_item_and_push(
        &self,
        space_id: Uuid,
        resource_id: Uuid,
        resource_kind: PimResourceKind,
    ) -> Result<()> {
        let (key_epoch, space_key) = self.current_space_key(space_id).await?;
        let identity = self
            .device_identity
            .as_ref()
            .ok_or_else(|| anyhow!("this device has not been approved for encrypted writes"))?;
        let projection_resource_id = default_resource_id(resource_kind, resource_id);
        let dependencies = self
            .load_resource_head(&space_id.to_string(), &projection_resource_id)
            .await?
            .into_iter()
            .collect();
        let operation = PimOperationV1::Delete(PimDeleteV1 {
            resource_kind,
            resource_id,
            dependencies,
            projection_resource_id: Some(projection_resource_id.clone()),
        });
        let client_op_id = Uuid::new_v4();
        let envelope = OperationEnvelopeV1::seal_xchacha(
            OperationSealContext {
                space_id,
                stream_id: resource_id,
                client_op_id,
                author_device_id: identity.device_id,
                key_epoch,
                envelope_kind: EnvelopeKind::Operation,
            },
            &operation.encode()?,
            &space_key,
            &SigningKey::from_bytes(&identity.signing_private_key),
        )?;
        self.queue_operation(envelope.clone(), now_unix_ms())
            .await?;
        self.delete_resource(
            dav_kind(resource_kind),
            space_id.to_string(),
            projection_resource_id.clone(),
        )
        .await?;
        self.store_operation_state(CachedOperationState {
            client_op_id,
            collection_id: space_id.to_string(),
            stream_id: resource_id,
            logical_resource_id: projection_resource_id.clone(),
            materialized_resource_id: projection_resource_id.clone(),
            kind: dav_kind(resource_kind),
            payload: None,
            deleted: true,
        })
        .await?;
        self.store_resource_head(space_id.to_string(), projection_resource_id, client_op_id)
            .await?;
        if self.cloud.append_operation(&envelope).await.is_ok() {
            self.remove_queued_operation(client_op_id).await?;
        }
        Ok(())
    }

    #[cfg(feature = "local-bridge")]
    pub(crate) async fn put_resource_and_push(
        &self,
        kind: DavResourceKind,
        collection_id: String,
        resource_id: String,
        payload: String,
        updated_at_ms: i64,
    ) -> Result<PutResult> {
        let space_id = Uuid::parse_str(&collection_id).context("invalid security-space id")?;
        let (key_epoch, space_key) = self.current_space_key(space_id).await?;
        let identity = self
            .device_identity
            .as_ref()
            .ok_or_else(|| anyhow!("this DAV bridge device has not been approved"))?;
        let stream_id = stable_stream_id(space_id, &resource_id);
        let dependencies = self
            .load_resource_head(&collection_id, &resource_id)
            .await?
            .into_iter()
            .collect();
        let resource_kind = pim_kind(kind, &payload);
        let operation = PimOperationV1::Upsert(PimUpsertV1 {
            resource_kind,
            resource_id: stream_id,
            dependencies,
            fields: BTreeMap::from([(
                "dav_resource_id".to_string(),
                PimValue::Text(resource_id.clone()),
            )]),
            raw_projection: payload.as_bytes().to_vec(),
        });
        let client_op_id = Uuid::new_v4();
        let envelope = OperationEnvelopeV1::seal_xchacha(
            OperationSealContext {
                space_id,
                stream_id,
                client_op_id,
                author_device_id: identity.device_id,
                key_epoch,
                envelope_kind: EnvelopeKind::Operation,
            },
            &operation.encode()?,
            &space_key,
            &SigningKey::from_bytes(&identity.signing_private_key),
        )?;
        self.queue_operation(envelope.clone(), updated_at_ms)
            .await?;
        let materialized_payload = payload.clone();
        let resource = LocalResource {
            kind,
            collection_id: collection_id.clone(),
            resource_id: resource_id.clone(),
            etag: compute_etag(payload.as_bytes()),
            payload,
            updated_at_ms,
        };
        let (outcome, resource) = self.upsert(resource).await?;
        self.store_operation_state(CachedOperationState {
            client_op_id,
            collection_id: collection_id.clone(),
            stream_id,
            logical_resource_id: resource_id.clone(),
            materialized_resource_id: resource_id.clone(),
            kind,
            payload: Some(materialized_payload),
            deleted: false,
        })
        .await?;
        self.store_resource_head(collection_id, resource_id, client_op_id)
            .await?;
        let pushed = self.cloud.append_operation(&envelope).await;
        let (cloud_space_seq, cloud_pushed) = match pushed {
            Ok(sequence) => {
                self.remove_queued_operation(client_op_id).await?;
                (Some(sequence), true)
            }
            Err(error) => {
                warn!(?error, %client_op_id, "DAV write retained in encrypted outbox");
                (None, false)
            }
        };
        Ok(PutResult {
            outcome,
            etag: resource.etag,
            cloud_space_seq,
            cloud_pushed,
        })
    }

    #[cfg(feature = "local-bridge")]
    pub(crate) async fn delete_resource_and_push(
        &self,
        kind: DavResourceKind,
        collection_id: String,
        resource_id: String,
    ) -> Result<bool> {
        let space_id = Uuid::parse_str(&collection_id).context("invalid security-space id")?;
        let (key_epoch, space_key) = self.current_space_key(space_id).await?;
        let identity = self
            .device_identity
            .as_ref()
            .ok_or_else(|| anyhow!("this DAV bridge device has not been approved"))?;
        let stream_id = stable_stream_id(space_id, &resource_id);
        let dependencies = self
            .load_resource_head(&collection_id, &resource_id)
            .await?
            .into_iter()
            .collect();
        let operation = PimOperationV1::Delete(PimDeleteV1 {
            resource_kind: match kind {
                DavResourceKind::Contact => PimResourceKind::Contact,
                DavResourceKind::Calendar => PimResourceKind::CalendarEvent,
                DavResourceKind::Note => {
                    return Err(anyhow!("notes are not part of the MVP DAV projection"));
                }
            },
            resource_id: stream_id,
            dependencies,
            projection_resource_id: Some(resource_id.clone()),
        });
        let client_op_id = Uuid::new_v4();
        let envelope = OperationEnvelopeV1::seal_xchacha(
            OperationSealContext {
                space_id,
                stream_id,
                client_op_id,
                author_device_id: identity.device_id,
                key_epoch,
                envelope_kind: EnvelopeKind::Operation,
            },
            &operation.encode()?,
            &space_key,
            &SigningKey::from_bytes(&identity.signing_private_key),
        )?;
        self.queue_operation(envelope.clone(), now_unix_ms())
            .await?;
        let deleted = self
            .delete_resource(kind, collection_id.clone(), resource_id.clone())
            .await?;
        self.store_operation_state(CachedOperationState {
            client_op_id,
            collection_id: collection_id.clone(),
            stream_id,
            logical_resource_id: resource_id.clone(),
            materialized_resource_id: resource_id.clone(),
            kind,
            payload: None,
            deleted: true,
        })
        .await?;
        self.store_resource_head(collection_id, resource_id, client_op_id)
            .await?;
        if self.cloud.append_operation(&envelope).await.is_ok() {
            self.remove_queued_operation(client_op_id).await?;
        }
        Ok(deleted)
    }

    async fn current_space_key(&self, space_id: Uuid) -> Result<(u32, [u8; 32])> {
        self.collection_keys
            .read()
            .await
            .get(&space_id)
            .and_then(|keys| keys.last_key_value().map(|(epoch, key)| (*epoch, *key)))
            .ok_or_else(|| anyhow!("missing current key for security space {space_id}"))
    }

    async fn queue_operation(&self, envelope: OperationEnvelopeV1, timestamp: i64) -> Result<()> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.queue_operation(&envelope, timestamp)).await?
    }

    async fn upsert_authoritative(&self, resource: LocalResource) -> Result<UpsertOutcome> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.upsert_authoritative(&resource)).await?
    }

    async fn remove_queued_operation(&self, operation_id: Uuid) -> Result<()> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.remove_queued_operation(operation_id)).await?
    }

    async fn flush_outbox(&self) -> Result<()> {
        let cache = self.cache.clone();
        let queued = tokio::task::spawn_blocking(move || cache.list_queued_operations()).await??;
        for envelope in queued {
            self.cloud.append_operation(&envelope).await?;
            self.remove_queued_operation(envelope.client_op_id).await?;
        }
        Ok(())
    }
}

#[cfg(feature = "local-bridge")]
fn pim_kind(kind: DavResourceKind, payload: &str) -> PimResourceKind {
    match kind {
        DavResourceKind::Contact => PimResourceKind::Contact,
        DavResourceKind::Calendar if payload.contains("BEGIN:VTODO") => PimResourceKind::Task,
        DavResourceKind::Calendar => PimResourceKind::CalendarEvent,
        DavResourceKind::Note => PimResourceKind::Task,
    }
}

fn dav_kind(kind: PimResourceKind) -> DavResourceKind {
    match kind {
        PimResourceKind::Contact => DavResourceKind::Contact,
        PimResourceKind::CalendarEvent | PimResourceKind::Task => DavResourceKind::Calendar,
    }
}

fn pim_kind_from_cached_state(state: &CachedOperationState) -> PimResourceKind {
    match state.kind {
        DavResourceKind::Contact => PimResourceKind::Contact,
        DavResourceKind::Calendar
            if state
                .payload
                .as_deref()
                .is_some_and(|payload| payload.contains("BEGIN:VTODO")) =>
        {
            PimResourceKind::Task
        }
        DavResourceKind::Calendar => PimResourceKind::CalendarEvent,
        DavResourceKind::Note => PimResourceKind::Task,
    }
}

#[cfg(feature = "local-bridge")]
fn stable_stream_id(space_id: Uuid, resource_id: &str) -> Uuid {
    Uuid::new_v5(&space_id, resource_id.as_bytes())
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn compute_etag(payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    hex::encode(hasher.finalize())
}

fn default_resource_id(kind: PimResourceKind, resource_id: Uuid) -> String {
    match kind {
        PimResourceKind::Contact => format!("{resource_id}.vcf"),
        PimResourceKind::CalendarEvent | PimResourceKind::Task => format!("{resource_id}.ics"),
    }
}

fn dav_resource_id(upsert: &PimUpsertV1) -> String {
    match upsert.fields.get("dav_resource_id") {
        Some(PimValue::Text(value)) if !value.trim().is_empty() => value.clone(),
        _ => default_resource_id(upsert.resource_kind, upsert.resource_id),
    }
}

fn text_field(upsert: &PimUpsertV1, name: &str) -> String {
    match upsert.fields.get(name) {
        Some(PimValue::Text(value)) => value.clone(),
        _ => String::new(),
    }
}

fn escape_projection(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace(',', "\\,")
        .replace(';', "\\;")
}

fn materialize_projection(upsert: &PimUpsertV1, existing: Option<&str>) -> String {
    if !upsert.raw_projection.is_empty() {
        return String::from_utf8_lossy(&upsert.raw_projection).into_owned();
    }
    if let Some(existing) = existing {
        return patch_projection(existing, upsert);
    }
    let uid = upsert.resource_id;
    let title = escape_projection(&text_field(upsert, "title"));
    match upsert.resource_kind {
        PimResourceKind::Contact => format!(
            "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:{uid}\r\nFN:{title}\r\nEMAIL:{}\r\nTEL:{}\r\nEND:VCARD\r\n",
            escape_projection(&text_field(upsert, "email")),
            escape_projection(&text_field(upsert, "phone")),
        ),
        PimResourceKind::CalendarEvent => format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Kamori//EN\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nSUMMARY:{title}\r\nDTSTART:{}\r\nDTEND:{}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            escape_projection(&text_field(upsert, "starts_at")),
            escape_projection(&text_field(upsert, "ends_at")),
        ),
        PimResourceKind::Task => {
            let completed = matches!(
                upsert.fields.get("completed"),
                Some(PimValue::Boolean(true))
            );
            format!(
                "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Kamori//EN\r\nBEGIN:VTODO\r\nUID:{uid}\r\nSUMMARY:{title}\r\nSTATUS:{}\r\nEND:VTODO\r\nEND:VCALENDAR\r\n",
                if completed {
                    "COMPLETED"
                } else {
                    "NEEDS-ACTION"
                },
            )
        }
    }
}

fn patch_projection(existing: &str, upsert: &PimUpsertV1) -> String {
    let mut edits = Vec::<(&str, String)>::new();
    if let Some(PimValue::Text(value)) = upsert.fields.get("title") {
        edits.push((
            if upsert.resource_kind == PimResourceKind::Contact {
                "FN"
            } else {
                "SUMMARY"
            },
            escape_projection(value),
        ));
    }
    if let Some(PimValue::Text(value)) = upsert.fields.get("email") {
        edits.push(("EMAIL", escape_projection(value)));
    }
    if let Some(PimValue::Text(value)) = upsert.fields.get("phone") {
        edits.push(("TEL", escape_projection(value)));
    }
    if let Some(PimValue::Text(value)) = upsert.fields.get("starts_at") {
        edits.push(("DTSTART", escape_projection(value)));
    }
    if let Some(PimValue::Text(value)) = upsert.fields.get("ends_at") {
        edits.push(("DTEND", escape_projection(value)));
    }
    if let Some(PimValue::Boolean(completed)) = upsert.fields.get("completed") {
        edits.push((
            "STATUS",
            if *completed {
                "COMPLETED".to_string()
            } else {
                "NEEDS-ACTION".to_string()
            },
        ));
    }
    if edits.is_empty() {
        return existing.to_string();
    }

    let mut applied = vec![false; edits.len()];
    let mut lines = Vec::new();
    for original_line in existing.lines() {
        let line = original_line.trim_end_matches('\r');
        let property_name = line
            .split_once(':')
            .map(|(head, _)| head.split(';').next().unwrap_or(head))
            .unwrap_or("");
        if let Some((index, (name, value))) = edits
            .iter()
            .enumerate()
            .find(|(index, (name, _))| !applied[*index] && property_name.eq_ignore_ascii_case(name))
        {
            lines.push(format!("{name}:{value}"));
            applied[index] = true;
        } else {
            lines.push(line.to_string());
        }
    }

    let end_marker = match upsert.resource_kind {
        PimResourceKind::Contact => "END:VCARD",
        PimResourceKind::CalendarEvent => "END:VEVENT",
        PimResourceKind::Task => "END:VTODO",
    };
    let insertion_index = lines
        .iter()
        .position(|line| line.eq_ignore_ascii_case(end_marker))
        .unwrap_or(lines.len());
    for (index, (name, value)) in edits.iter().enumerate().rev() {
        if !applied[index] {
            lines.insert(insertion_index, format!("{name}:{value}"));
        }
    }
    format!("{}\r\n", lines.join("\r\n"))
}

#[cfg(test)]
mod first_party_pim_tests {
    use super::*;

    #[test]
    fn partial_task_edit_preserves_existing_and_extension_properties() {
        let upsert = PimUpsertV1 {
            resource_kind: PimResourceKind::Task,
            resource_id: Uuid::from_u128(1),
            dependencies: Vec::new(),
            fields: BTreeMap::from([("completed".to_string(), PimValue::Boolean(true))]),
            raw_projection: Vec::new(),
        };
        let existing = "BEGIN:VCALENDAR\r\nBEGIN:VTODO\r\nSUMMARY:Keep me\r\nX-KAMORI-UNKNOWN:value\r\nSTATUS:NEEDS-ACTION\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";
        let materialized = materialize_projection(&upsert, Some(existing));
        assert!(materialized.contains("SUMMARY:Keep me\r\n"));
        assert!(materialized.contains("X-KAMORI-UNKNOWN:value\r\n"));
        assert!(materialized.contains("STATUS:COMPLETED\r\n"));
        assert!(!materialized.contains("STATUS:NEEDS-ACTION"));
    }

    #[tokio::test]
    async fn first_party_write_is_materialized_and_queued_offline() {
        let space_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();
        let mut db_path = std::env::temp_dir();
        db_path.push(format!(
            "kamori_native_pim_{}_{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let config =
            LocalBridgeConfig::new(db_path.clone(), "http://127.0.0.1:9", "offline-test-token")
                .with_device_identity(LocalDeviceIdentity {
                    device_id: Uuid::new_v4(),
                    signing_private_key: [7_u8; 32],
                });
        let runner = LocalBridgeRunner::new(config).expect("runner");
        runner
            .register_collection_key_epoch(space_id.to_string(), 1, [9_u8; 32])
            .await;

        runner
            .upsert_pim_item(
                space_id,
                resource_id,
                PimResourceKind::Task,
                BTreeMap::from([
                    ("title".to_string(), PimValue::Text("Ship MVP".to_string())),
                    ("completed".to_string(), PimValue::Boolean(false)),
                ]),
            )
            .await
            .expect("offline write");

        let resources = runner
            .list_cached_resources(space_id, DavResourceKind::Calendar)
            .await
            .expect("list cache");
        assert_eq!(resources.len(), 1);
        assert!(resources[0].payload.contains("SUMMARY:Ship MVP"));
        assert_eq!(
            runner.state.cache.list_queued_operations().unwrap().len(),
            1
        );

        runner
            .delete_pim_item(space_id, resource_id, PimResourceKind::Task)
            .await
            .expect("offline delete");
        assert!(
            runner
                .list_cached_resources(space_id, DavResourceKind::Calendar)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            runner.state.cache.list_queued_operations().unwrap().len(),
            2
        );

        let _ = std::fs::remove_file(db_path);
    }
}
