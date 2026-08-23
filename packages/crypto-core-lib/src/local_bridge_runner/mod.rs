//! Encrypted local sync runtime with an optional desktop-only DAV projection.

mod cache;
mod cloud;
#[cfg(feature = "local-bridge")]
mod dav;
mod types;

pub use types::{
    DavResourceKind, LocalBridgeConfig, LocalDeviceIdentity, LocalResource, MaterializedPimBranch,
    UpsertOutcome,
};

use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
#[cfg(feature = "local-bridge")]
use axum::{Router, extract::DefaultBodyLimit, routing::any};
use cache::{CachedOperationState, LocalCache};
use cloud::CloudSyncClient;
#[cfg(feature = "local-bridge")]
use dav::dav_dispatch;
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};
#[cfg(feature = "local-bridge")]
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};
use tracing::warn;
#[cfg(feature = "local-bridge")]
use tracing::{error, info};
use url::{Host, Url};
use uuid::Uuid;

use crate::{
    operation_envelope::{EnvelopeKind, OperationEnvelopeV1, OperationSealContext},
    pim::{
        PimDeleteV1, PimOperationV1, PimResourceKind, PimSnapshotBranchV2, PimSnapshotV2,
        PimUpsertV1, PimValue, materialize_projection,
    },
};

#[cfg(feature = "local-bridge")]
const MAX_DAV_REQUEST_BYTES: usize = 960 * 1024;

#[cfg(feature = "local-bridge")]
#[derive(Debug, thiserror::Error)]
pub(crate) enum DavWriteError {
    #[error("If-Match is required for an existing DAV resource")]
    PreconditionRequired,
    #[error("DAV resource precondition failed")]
    PreconditionFailed,
    #[error("DAV resource was not found")]
    NotFound,
}

#[cfg(feature = "local-bridge")]
#[derive(Clone, Debug)]
pub(crate) enum DavIfMatch {
    Any,
    StrongTags(Vec<String>),
}

#[cfg(feature = "local-bridge")]
impl DavIfMatch {
    fn matches(&self, current_etag: &str) -> bool {
        match self {
            Self::Any => true,
            Self::StrongTags(tags) => tags.iter().any(|tag| tag == current_etag),
        }
    }
}

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
        let cloud_base_url = normalize_cloud_base_url(&cloud_base_url)?;
        let cache = LocalCache::new(sqlite_path, sqlite_key)?;
        let (access_token, refresh_token) = cache
            .recover_rotated_credentials(refresh_token.as_deref())?
            .unwrap_or((access_token, refresh_token.unwrap_or_default()));
        let refresh_token = (!refresh_token.is_empty()).then_some(refresh_token);
        let attempt_cache = cache.clone();
        let rotation_attempt_source =
            Arc::new(move |refresh: &str| attempt_cache.begin_refresh_rotation(refresh));
        let token_cache = cache.clone();
        let rotation_sink = Arc::new(move |previous: &str, access: &str, refresh: &str| {
            token_cache.store_rotated_credentials(previous, access, refresh)
        });
        Ok(Self {
            state: Arc::new(LocalBridgeState {
                cache,
                collection_keys: RwLock::new(HashMap::new()),
                cloud: CloudSyncClient::new(
                    cloud_base_url,
                    access_token,
                    refresh_token,
                    rotation_attempt_source,
                    rotation_sink,
                )?,
                device_identity,
                materialization_lock: Mutex::new(()),
                outbox_flush_lock: Mutex::new(()),
                sync_cycle_lock: Mutex::new(()),
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
            .layer(DefaultBodyLimit::max(MAX_DAV_REQUEST_BYTES))
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
        lifecycle.local_addr = Some(addr);
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
        lifecycle.local_addr = None;
        Ok(())
    }

    #[cfg(feature = "local-bridge")]
    pub async fn is_running(&self) -> bool {
        self.lifecycle.lock().await.handle.is_some()
    }

    /// Returns the address currently used by the embedded DAV listener.
    ///
    /// This can differ from [`Self::bind_addr`] when the configured port is
    /// zero and the operating system selects an available ephemeral port.
    #[cfg(feature = "local-bridge")]
    pub async fn local_addr(&self) -> Option<std::net::SocketAddr> {
        self.lifecycle.lock().await.local_addr
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
        if let Err(error) = self
            .register_collection_key_epoch_from(collection_id, key_epoch, key, 0)
            .await
        {
            warn!(%error, "ignored invalid security-space key registration");
        }
    }

    /// Registers a key and raises the local cursor to a server-authenticated
    /// sync boundary. The boundary may come from membership history or from a
    /// complete current-epoch snapshot checkpoint.
    pub async fn register_collection_key_epoch_from(
        &self,
        collection_id: impl Into<String>,
        key_epoch: u32,
        key: [u8; 32],
        sync_start_seq: u64,
    ) -> Result<()> {
        let collection_id = collection_id.into();
        let space_id = Uuid::parse_str(&collection_id).context("invalid security-space id")?;
        anyhow::ensure!(key_epoch > 0, "key epoch must be positive");
        self.state
            .collection_keys
            .write()
            .await
            .entry(space_id)
            .or_default()
            .insert(key_epoch, key);
        let cache = self.state.cache.clone();
        let scope = format!("space:{space_id}");
        tokio::task::spawn_blocking(move || cache.advance_last_seq(&scope, sync_start_seq))
            .await??;
        Ok(())
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
        Ok(self
            .sync_once_by_space()
            .await?
            .values()
            .copied()
            .fold(0_u64, u64::saturating_add))
    }

    /// Runs one serialized sync cycle and reports materialized operations by
    /// security space so multi-collection clients do not attribute all work to
    /// an arbitrary collection.
    pub async fn sync_once_by_space(&self) -> Result<BTreeMap<Uuid, u64>> {
        let _sync_guard = self.state.sync_cycle_lock.lock().await;
        let spaces = self.state.collection_keys.read().await.clone();
        let mut applied = BTreeMap::new();
        for (space_id, keys) in spaces {
            self.state.flush_outbox(space_id).await?;
            applied.insert(space_id, self.state.sync_space(space_id, &keys).await?);
        }
        Ok(applied)
    }

    pub async fn current_refresh_token(&self) -> Option<String> {
        self.state.cloud.current_refresh_token().await
    }

    pub async fn current_access_token(&self) -> String {
        self.state.cloud.current_access_token().await
    }

    /// Clears crash-recovery copies of rotated credentials during logout or an
    /// account/backend switch.
    pub async fn clear_persisted_credentials(&self) -> Result<()> {
        let cache = self.state.cache.clone();
        tokio::task::spawn_blocking(move || cache.clear_runtime_credentials()).await?
    }

    /// Persists a rotation performed by another authenticated transport which
    /// shares this encrypted runtime cache.
    pub async fn persist_rotated_credentials(
        &self,
        previous_refresh_token: String,
        access_token: String,
        refresh_token: String,
    ) -> Result<()> {
        let cache = self.state.cache.clone();
        tokio::task::spawn_blocking(move || {
            cache.store_rotated_credentials(&previous_refresh_token, &access_token, &refresh_token)
        })
        .await?
    }

    /// Performs an authenticated MessagePack request through the runner token
    /// manager, including serialized refresh-token rotation on HTTP 401.
    pub async fn cloud_get_msgpack(&self, path: &str) -> Result<Vec<u8>> {
        self.state.cloud.get_msgpack(path).await
    }

    /// Performs an authenticated MessagePack POST through the runner token
    /// manager, including serialized refresh-token rotation on HTTP 401.
    pub async fn cloud_post_msgpack(&self, path: &str, body: Vec<u8>) -> Result<Vec<u8>> {
        self.state.cloud.post_msgpack(path, body).await
    }

    pub async fn space_cursor(&self, space_id: Uuid) -> Result<u64> {
        self.state.load_last_seq(&format!("space:{space_id}")).await
    }

    pub async fn quarantined_stream_ids(
        &self,
        space_id: Uuid,
        through_space_seq: u64,
    ) -> Result<Vec<Uuid>> {
        let cache = self.state.cache.clone();
        tokio::task::spawn_blocking(move || {
            cache.quarantined_stream_ids(space_id, through_space_seq)
        })
        .await?
    }

    /// Lists decrypted cached resources for a first-party client projection.
    pub async fn list_cached_resources(
        &self,
        space_id: Uuid,
        kind: DavResourceKind,
    ) -> Result<Vec<LocalResource>> {
        self.state.list_resources(kind, space_id.to_string()).await
    }

    /// Lists explicit branch identities for first-party PIM clients.
    pub async fn list_materialized_pim_branches(
        &self,
        space_id: Uuid,
    ) -> Result<Vec<MaterializedPimBranch>> {
        self.state.list_materialized_pim_branches(space_id).await
    }

    /// Writes a field-level first-party PIM operation and retains it in the
    /// encrypted outbox when the cloud is temporarily unavailable.
    pub async fn upsert_pim_item(
        &self,
        space_id: Uuid,
        resource_id: Uuid,
        resource_kind: PimResourceKind,
        fields: BTreeMap<String, PimValue>,
    ) -> Result<Uuid> {
        self.state
            .upsert_pim_item_and_push(space_id, resource_id, resource_kind, fields, None, None)
            .await
    }

    /// Mutates one known materialized branch using optimistic concurrency.
    pub async fn upsert_pim_branch(
        &self,
        space_id: Uuid,
        resource_id: Uuid,
        projection_resource_id: String,
        expected_head_operation_id: Uuid,
        resource_kind: PimResourceKind,
        fields: BTreeMap<String, PimValue>,
    ) -> Result<Uuid> {
        self.state
            .upsert_pim_item_and_push(
                space_id,
                resource_id,
                resource_kind,
                fields,
                Some(projection_resource_id),
                Some(expected_head_operation_id),
            )
            .await
    }

    /// Tombstones a first-party PIM item and queues the signed operation.
    pub async fn delete_pim_item(
        &self,
        space_id: Uuid,
        resource_id: Uuid,
        resource_kind: PimResourceKind,
    ) -> Result<Uuid> {
        self.state
            .delete_pim_item_and_push(space_id, resource_id, resource_kind, None, None)
            .await
    }

    /// Tombstones one known branch using optimistic concurrency.
    pub async fn delete_pim_branch(
        &self,
        space_id: Uuid,
        resource_id: Uuid,
        projection_resource_id: String,
        expected_head_operation_id: Uuid,
        resource_kind: PimResourceKind,
    ) -> Result<Uuid> {
        self.state
            .delete_pim_item_and_push(
                space_id,
                resource_id,
                resource_kind,
                Some(projection_resource_id),
                Some(expected_head_operation_id),
            )
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

/// Normalizes a Kamori cloud endpoint to a path-free HTTPS origin.
///
/// Cleartext is accepted only for an IP/domain loopback origin so local test
/// stacks remain usable without weakening remote endpoint validation.
pub fn normalize_cloud_base_url(value: &str) -> Result<String> {
    let parsed = Url::parse(value.trim()).context("parse Kamori cloud URL")?;
    let loopback = match parsed.host() {
        Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    anyhow::ensure!(
        parsed.scheme() == "https" || (parsed.scheme() == "http" && loopback),
        "remote Kamori cloud URLs must use HTTPS"
    );
    anyhow::ensure!(
        parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.query().is_none()
            && parsed.fragment().is_none(),
        "Kamori cloud URL must not contain credentials, query, or fragment"
    );
    anyhow::ensure!(
        parsed.path().bytes().all(|byte| byte == b'/'),
        "Kamori cloud URL must be an origin without a path"
    );
    Ok(parsed.origin().ascii_serialization())
}

#[cfg(feature = "local-bridge")]
#[derive(Default)]
struct ServerLifecycle {
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
    local_addr: Option<std::net::SocketAddr>,
}

#[cfg(feature = "local-bridge")]
#[derive(Debug)]
pub(crate) struct PutResult {
    pub(crate) outcome: UpsertOutcome,
    pub(crate) etag: String,
    pub(crate) cloud_space_seq: Option<u64>,
    pub(crate) cloud_pushed: bool,
}

#[cfg(feature = "local-bridge")]
pub(crate) struct DavPutRequest {
    pub(crate) kind: DavResourceKind,
    pub(crate) collection_id: String,
    pub(crate) resource_id: String,
    pub(crate) payload: String,
    pub(crate) updated_at_ms: i64,
    pub(crate) if_match: Option<DavIfMatch>,
    pub(crate) require_absence: bool,
}

pub(crate) struct LocalBridgeState {
    cache: LocalCache,
    collection_keys: RwLock<HashMap<Uuid, BTreeMap<u32, [u8; 32]>>>,
    cloud: CloudSyncClient,
    device_identity: Option<LocalDeviceIdentity>,
    materialization_lock: Mutex<()>,
    outbox_flush_lock: Mutex<()>,
    sync_cycle_lock: Mutex<()>,
    #[cfg(feature = "local-bridge")]
    dav_credentials: Option<(String, String)>,
}

impl LocalBridgeState {
    async fn list_materialized_pim_branches(
        &self,
        space_id: Uuid,
    ) -> Result<Vec<MaterializedPimBranch>> {
        let cache = self.cache.clone();
        let collection_id = space_id.to_string();
        let states = tokio::task::spawn_blocking(move || {
            cache.list_materialized_head_states(&collection_id)
        })
        .await??;
        let mut branch_counts = HashMap::<Uuid, usize>::new();
        for state in &states {
            *branch_counts.entry(state.stream_id).or_default() += 1;
        }
        Ok(states
            .into_iter()
            .map(|state| MaterializedPimBranch {
                space_id,
                logical_resource_id: state.stream_id,
                conflict: branch_counts.get(&state.stream_id).copied().unwrap_or(0) > 1,
                projection_resource_id: state.materialized_resource_id,
                head_operation_id: state.client_op_id,
                kind: state.kind,
                payload: state.payload,
                deleted: state.deleted,
            })
            .collect())
    }

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
        let mut applied = self
            .retry_unresolved_pim_operations(space_id, keys, &directory)
            .await?;
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
                    let plaintext = match stored.envelope.open(key) {
                        Ok(plaintext) => plaintext,
                        Err(error) => {
                            tracing::warn!(
                                %space_id,
                                client_op_id = %stored.envelope.client_op_id,
                                space_seq = stored.space_seq,
                                %error,
                                "quarantined authenticated operation with invalid ciphertext"
                            );
                            self.quarantine_operation(
                                stored.envelope.clone(),
                                stored.space_seq,
                                "invalid_ciphertext",
                            )
                            .await?;
                            continue;
                        }
                    };
                    match stored.envelope.envelope_kind {
                        EnvelopeKind::Operation => {
                            let operation = match PimOperationV1::decode(&plaintext) {
                                Ok(operation)
                                    if operation_resource_id(&operation)
                                        == stored.envelope.stream_id =>
                                {
                                    operation
                                }
                                Ok(_) => {
                                    self.quarantine_operation(
                                        stored.envelope.clone(),
                                        stored.space_seq,
                                        "operation_stream_mismatch",
                                    )
                                    .await?;
                                    continue;
                                }
                                Err(error) => {
                                    tracing::warn!(
                                        %space_id,
                                        client_op_id = %stored.envelope.client_op_id,
                                        space_seq = stored.space_seq,
                                        %error,
                                        "quarantined invalid authenticated PIM operation"
                                    );
                                    self.quarantine_operation(
                                        stored.envelope.clone(),
                                        stored.space_seq,
                                        "invalid_operation",
                                    )
                                    .await?;
                                    continue;
                                }
                            };
                            if let Err(error) = self
                                .apply_pim_operation(
                                    space_id,
                                    stored.space_seq,
                                    &stored.envelope,
                                    operation,
                                )
                                .await
                            {
                                tracing::warn!(
                                    %space_id,
                                    client_op_id = %stored.envelope.client_op_id,
                                    space_seq = stored.space_seq,
                                    %error,
                                    "quarantined operation with an unresolved PIM graph"
                                );
                                self.quarantine_operation(
                                    stored.envelope.clone(),
                                    stored.space_seq,
                                    "unresolved_pim_graph",
                                )
                                .await?;
                                continue;
                            }
                            applied = applied.saturating_add(1);
                        }
                        EnvelopeKind::Snapshot => {
                            let snapshot = match PimSnapshotV2::decode(&plaintext) {
                                Ok(snapshot)
                                    if snapshot.resource_id == stored.envelope.stream_id
                                        && snapshot.covers_through_space_seq
                                            <= stored.space_seq =>
                                {
                                    snapshot
                                }
                                Ok(_) => {
                                    self.quarantine_operation(
                                        stored.envelope.clone(),
                                        stored.space_seq,
                                        "invalid_snapshot_context",
                                    )
                                    .await?;
                                    continue;
                                }
                                Err(error) => {
                                    tracing::warn!(
                                        %space_id,
                                        client_op_id = %stored.envelope.client_op_id,
                                        space_seq = stored.space_seq,
                                        %error,
                                        "quarantined invalid authenticated PIM snapshot"
                                    );
                                    self.quarantine_operation(
                                        stored.envelope.clone(),
                                        stored.space_seq,
                                        "invalid_snapshot",
                                    )
                                    .await?;
                                    continue;
                                }
                            };
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
                    return Err(anyhow!(
                        "missing security-space key epoch {} before cursor {}",
                        stored.envelope.key_epoch,
                        stored.space_seq
                    ));
                }
            }
            applied = applied.saturating_add(
                self.retry_unresolved_pim_operations(space_id, keys, &directory)
                    .await?,
            );
            if operation_count == 0 || page.next_cursor <= cursor {
                break;
            }
            cursor = page.next_cursor;
            self.store_last_seq(&scope, cursor).await?;
        }
        Ok(applied)
    }

    async fn retry_unresolved_pim_operations(
        &self,
        space_id: Uuid,
        keys: &BTreeMap<u32, [u8; 32]>,
        directory: &HashMap<Uuid, Vec<u8>>,
    ) -> Result<u64> {
        let mut applied = 0_u64;
        loop {
            let cache = self.cache.clone();
            let pending = tokio::task::spawn_blocking(move || {
                cache.quarantined_operations(space_id, "unresolved_pim_graph")
            })
            .await??;
            if pending.is_empty() {
                break;
            }
            let mut made_progress = false;
            for (envelope, space_seq) in pending {
                let Some(public_key) = directory.get(&envelope.author_device_id) else {
                    continue;
                };
                if envelope.verify(public_key).is_err() {
                    continue;
                }
                let Some(key) = keys.get(&envelope.key_epoch) else {
                    continue;
                };
                let Ok(plaintext) = envelope.open(key) else {
                    continue;
                };
                let Ok(operation) = PimOperationV1::decode(&plaintext) else {
                    continue;
                };
                if operation_resource_id(&operation) != envelope.stream_id
                    || self
                        .apply_pim_operation(space_id, space_seq, &envelope, operation)
                        .await
                        .is_err()
                {
                    continue;
                }
                let cache = self.cache.clone();
                let client_op_id = envelope.client_op_id;
                tokio::task::spawn_blocking(move || {
                    cache.remove_quarantined_operation(space_id, client_op_id)
                })
                .await??;
                applied = applied.saturating_add(1);
                made_progress = true;
            }
            if !made_progress {
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
        let operation_stream_id = match &operation {
            PimOperationV1::Upsert(value) => value.resource_id,
            PimOperationV1::Delete(value) => value.resource_id,
        };
        anyhow::ensure!(
            operation_stream_id == envelope.stream_id,
            "operation stream mismatch"
        );
        let _materialization_guard = self.materialization_lock.lock().await;
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
            .load_operation_state(collection_id.clone(), envelope.client_op_id)
            .await?
            .is_some()
        {
            self.acknowledge_operation(collection_id, envelope.client_op_id, space_seq)
                .await?;
            return Ok(());
        }
        let mut dependency_states = Vec::new();
        for dependency in dependencies {
            if let Some(state) = self
                .load_operation_state(collection_id.clone(), *dependency)
                .await?
            {
                dependency_states.push(state);
            }
        }
        anyhow::ensure!(
            dependency_states.len() == dependencies.len(),
            "PIM operation dependency is missing"
        );
        let parent_operation_id = dependencies.first().copied();
        let base_state = dependency_states.first();
        let canonical_resource_id = base_state
            .map(|state| state.logical_resource_id.clone())
            .unwrap_or(resource_id);
        if matches!(operation, PimOperationV1::Delete(_)) {
            let operation_state = CachedOperationState {
                client_op_id: envelope.client_op_id,
                space_seq,
                collection_id: collection_id.clone(),
                stream_id: envelope.stream_id,
                logical_resource_id: canonical_resource_id.clone(),
                materialized_resource_id: canonical_resource_id.clone(),
                kind: dav_kind,
                payload: None,
                deleted: true,
                parent_operation_id,
                seed_projection_resource_id: None,
            };
            self.store_operation_state(operation_state).await?;
            self.reconcile_pim_stream(
                collection_id,
                envelope.stream_id,
                canonical_resource_id,
                dav_kind,
            )
            .await?;
            return Ok(());
        }
        let PimOperationV1::Upsert(upsert) = operation else {
            anyhow::bail!("unsupported PIM operation variant");
        };
        let existing_payload = base_state
            .and_then(|state| state.payload.as_deref())
            .map(str::to_owned);
        let payload = materialize_projection(&upsert, existing_payload.as_deref())?;
        let operation_state = CachedOperationState {
            client_op_id: envelope.client_op_id,
            space_seq,
            collection_id: collection_id.clone(),
            stream_id: envelope.stream_id,
            logical_resource_id: canonical_resource_id.clone(),
            materialized_resource_id: canonical_resource_id.clone(),
            kind: dav_kind,
            payload: Some(payload),
            deleted: false,
            parent_operation_id,
            seed_projection_resource_id: None,
        };
        self.store_operation_state(operation_state).await?;
        self.reconcile_pim_stream(
            collection_id,
            envelope.stream_id,
            canonical_resource_id,
            dav_kind,
        )
        .await?;
        Ok(())
    }

    async fn apply_pim_snapshot(
        &self,
        space_id: Uuid,
        space_seq: u64,
        envelope: &OperationEnvelopeV1,
        snapshot: PimSnapshotV2,
    ) -> Result<()> {
        anyhow::ensure!(
            snapshot.resource_id == envelope.stream_id,
            "snapshot stream mismatch"
        );
        anyhow::ensure!(
            snapshot.covers_through_space_seq <= space_seq,
            "snapshot coverage exceeds its transport position"
        );
        let _materialization_guard = self.materialization_lock.lock().await;
        let collection_id = space_id.to_string();
        let kind = dav_kind(snapshot.resource_kind);
        let logical_resource_id = default_resource_id(snapshot.resource_kind, snapshot.resource_id);
        for branch in snapshot.branches {
            if self
                .load_operation_state(collection_id.clone(), branch.head_operation_id)
                .await?
                .is_some()
            {
                self.acknowledge_operation(
                    collection_id.clone(),
                    branch.head_operation_id,
                    snapshot.covers_through_space_seq,
                )
                .await?;
                continue;
            }
            if let Some(head_id) = self
                .load_resource_head(&collection_id, &branch.projection_resource_id)
                .await?
                && let Some(head) = self
                    .load_operation_state(collection_id.clone(), head_id)
                    .await?
                && (head.space_seq == 0 || head.space_seq > snapshot.covers_through_space_seq)
            {
                tracing::debug!(
                    %space_id,
                    stream_id = %envelope.stream_id,
                    projection_resource_id = %branch.projection_resource_id,
                    snapshot_coverage = snapshot.covers_through_space_seq,
                    current_head_seq = head.space_seq,
                    "ignored snapshot branch older than the current materialized head"
                );
                continue;
            }
            let payload = if branch.deleted {
                self.delete_resource(
                    kind,
                    collection_id.clone(),
                    branch.projection_resource_id.clone(),
                )
                .await?;
                None
            } else {
                let projection = String::from_utf8(branch.materialized_projection)
                    .context("snapshot projection is not UTF-8")?;
                self.upsert_authoritative(LocalResource {
                    kind,
                    collection_id: collection_id.clone(),
                    resource_id: branch.projection_resource_id.clone(),
                    etag: compute_etag(projection.as_bytes()),
                    payload: projection.clone(),
                    updated_at_ms: i64::try_from(space_seq).unwrap_or(i64::MAX),
                })
                .await?;
                Some(projection)
            };
            let projection_resource_id = branch.projection_resource_id;
            self.store_operation_state_and_head(
                CachedOperationState {
                    client_op_id: branch.head_operation_id,
                    space_seq: snapshot.covers_through_space_seq,
                    collection_id: collection_id.clone(),
                    stream_id: envelope.stream_id,
                    logical_resource_id: logical_resource_id.clone(),
                    materialized_resource_id: projection_resource_id.clone(),
                    kind,
                    payload,
                    deleted: branch.deleted,
                    parent_operation_id: None,
                    seed_projection_resource_id: Some(projection_resource_id.clone()),
                },
                projection_resource_id,
            )
            .await?;
        }
        self.reconcile_pim_stream(collection_id, envelope.stream_id, logical_resource_id, kind)
            .await?;
        Ok(())
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

    async fn load_operation_state(
        &self,
        collection_id: String,
        operation_id: Uuid,
    ) -> Result<Option<CachedOperationState>> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || {
            cache.load_operation_state(&collection_id, operation_id)
        })
        .await?
    }

    async fn store_operation_state(&self, state: CachedOperationState) -> Result<()> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.store_operation_state(&state)).await?
    }

    async fn store_operation_state_and_head(
        &self,
        state: CachedOperationState,
        resource_id: String,
    ) -> Result<()> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || {
            cache.store_operation_state_and_head(&state, &resource_id)
        })
        .await?
    }

    async fn reconcile_pim_stream(
        &self,
        collection_id: String,
        stream_id: Uuid,
        default_projection_resource_id: String,
        kind: DavResourceKind,
    ) -> Result<()> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || {
            cache.reconcile_pim_stream(
                &collection_id,
                stream_id,
                &default_projection_resource_id,
                kind,
            )
        })
        .await?
    }

    async fn acknowledge_operation(
        &self,
        collection_id: String,
        operation_id: Uuid,
        space_seq: u64,
    ) -> Result<()> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || {
            cache.acknowledge_operation(&collection_id, operation_id, space_seq)
        })
        .await?
    }

    async fn quarantine_operation(
        &self,
        envelope: OperationEnvelopeV1,
        space_seq: u64,
        reason_code: &'static str,
    ) -> Result<()> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || {
            cache.quarantine_operation(&envelope, space_seq, reason_code)
        })
        .await?
    }

    async fn build_rotation_snapshots(
        &self,
        space_id: Uuid,
        new_key_epoch: u32,
        new_space_key: [u8; 32],
        covers_through_space_seq: u64,
        identity: &LocalDeviceIdentity,
    ) -> Result<Vec<OperationEnvelopeV1>> {
        let _materialization_guard = self.materialization_lock.lock().await;
        let cache = self.cache.clone();
        let collection_id = space_id.to_string();
        let states = tokio::task::spawn_blocking(move || {
            cache.list_materialized_head_states(&collection_id)
        })
        .await??;
        anyhow::ensure!(
            states.iter().all(|state| state.space_seq > 0),
            "all local operations must be acknowledged before key rotation"
        );
        let signing_key = SigningKey::from_bytes(&identity.signing_private_key);
        let mut streams = BTreeMap::<Uuid, Vec<CachedOperationState>>::new();
        for state in states {
            streams.entry(state.stream_id).or_default().push(state);
        }
        streams
            .into_iter()
            .map(|(stream_id, states)| {
                let first = states
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("snapshot stream contains no branches"))?;
                let resource_kind = pim_kind_from_cached_state(first);
                anyhow::ensure!(
                    states
                        .iter()
                        .all(|state| pim_kind_from_cached_state(state) == resource_kind),
                    "snapshot stream contains mixed PIM resource kinds"
                );
                let snapshot = PimSnapshotV2 {
                    schema_version: PimSnapshotV2::SCHEMA_VERSION,
                    covers_through_space_seq,
                    resource_kind,
                    resource_id: stream_id,
                    branches: states
                        .into_iter()
                        .map(|state| PimSnapshotBranchV2 {
                            projection_resource_id: state.materialized_resource_id,
                            head_operation_id: state.client_op_id,
                            deleted: state.deleted,
                            materialized_projection: state.payload.unwrap_or_default().into_bytes(),
                        })
                        .collect(),
                };
                OperationEnvelopeV1::seal_xchacha(
                    OperationSealContext {
                        space_id,
                        stream_id,
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
        mut fields: BTreeMap<String, PimValue>,
        projection_resource_id: Option<String>,
        expected_head_operation_id: Option<Uuid>,
    ) -> Result<Uuid> {
        let materialization_guard = self.materialization_lock.lock().await;
        let (key_epoch, space_key) = self.current_space_key(space_id).await?;
        let identity = self
            .device_identity
            .as_ref()
            .ok_or_else(|| anyhow!("this device has not been approved for encrypted writes"))?;
        let projection_resource_id = projection_resource_id
            .unwrap_or_else(|| default_resource_id(resource_kind, resource_id));
        validate_projection_resource_id(resource_kind, &projection_resource_id)?;
        let current_head = self
            .load_resource_head(&space_id.to_string(), &projection_resource_id)
            .await?;
        if let Some(expected_head) = expected_head_operation_id {
            anyhow::ensure!(
                current_head == Some(expected_head),
                "PIM branch changed since it was loaded; sync and retry"
            );
        }
        let dependencies = current_head.into_iter().collect();
        if resource_kind != PimResourceKind::Contact && !fields.contains_key("dtstamp") {
            fields.insert("dtstamp".to_string(), PimValue::Text(current_ical_utc()?));
        }
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
        )?;
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
        self.queue_operation(envelope, timestamp).await?;
        self.store_operation_state(CachedOperationState {
            client_op_id,
            space_seq: 0,
            collection_id: space_id.to_string(),
            stream_id: resource_id,
            logical_resource_id: projection_resource_id.clone(),
            materialized_resource_id: projection_resource_id.clone(),
            kind: dav_kind(resource_kind),
            payload: Some(payload),
            deleted: false,
            parent_operation_id: current_head,
            seed_projection_resource_id: None,
        })
        .await?;
        self.reconcile_pim_stream(
            space_id.to_string(),
            resource_id,
            default_resource_id(resource_kind, resource_id),
            dav_kind(resource_kind),
        )
        .await?;
        drop(materialization_guard);
        let _ = self.flush_outbox(space_id).await;
        Ok(client_op_id)
    }

    async fn delete_pim_item_and_push(
        &self,
        space_id: Uuid,
        resource_id: Uuid,
        resource_kind: PimResourceKind,
        projection_resource_id: Option<String>,
        expected_head_operation_id: Option<Uuid>,
    ) -> Result<Uuid> {
        let materialization_guard = self.materialization_lock.lock().await;
        let (key_epoch, space_key) = self.current_space_key(space_id).await?;
        let identity = self
            .device_identity
            .as_ref()
            .ok_or_else(|| anyhow!("this device has not been approved for encrypted writes"))?;
        let projection_resource_id = projection_resource_id
            .unwrap_or_else(|| default_resource_id(resource_kind, resource_id));
        validate_projection_resource_id(resource_kind, &projection_resource_id)?;
        let current_head = self
            .load_resource_head(&space_id.to_string(), &projection_resource_id)
            .await?;
        if let Some(expected_head) = expected_head_operation_id {
            anyhow::ensure!(
                current_head == Some(expected_head),
                "PIM branch changed since it was loaded; sync and retry"
            );
        }
        let dependencies = current_head.into_iter().collect();
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
        self.queue_operation(envelope, now_unix_ms()).await?;
        self.store_operation_state(CachedOperationState {
            client_op_id,
            space_seq: 0,
            collection_id: space_id.to_string(),
            stream_id: resource_id,
            logical_resource_id: projection_resource_id.clone(),
            materialized_resource_id: projection_resource_id.clone(),
            kind: dav_kind(resource_kind),
            payload: None,
            deleted: true,
            parent_operation_id: current_head,
            seed_projection_resource_id: None,
        })
        .await?;
        self.reconcile_pim_stream(
            space_id.to_string(),
            resource_id,
            default_resource_id(resource_kind, resource_id),
            dav_kind(resource_kind),
        )
        .await?;
        drop(materialization_guard);
        let _ = self.flush_outbox(space_id).await;
        Ok(client_op_id)
    }

    #[cfg(feature = "local-bridge")]
    pub(crate) async fn put_resource_and_push(&self, request: DavPutRequest) -> Result<PutResult> {
        let DavPutRequest {
            kind,
            collection_id,
            resource_id,
            payload,
            updated_at_ms,
            if_match,
            require_absence,
        } = request;
        let materialization_guard = self.materialization_lock.lock().await;
        let space_id = Uuid::parse_str(&collection_id).context("invalid security-space id")?;
        let (key_epoch, space_key) = self.current_space_key(space_id).await?;
        let identity = self
            .device_identity
            .as_ref()
            .ok_or_else(|| anyhow!("this DAV bridge device has not been approved"))?;
        let existing_resource = self
            .get_resource(kind, collection_id.clone(), resource_id.clone())
            .await?;
        match existing_resource.as_ref() {
            Some(existing) => {
                if require_absence {
                    return Err(DavWriteError::PreconditionFailed.into());
                }
                let expected = if_match
                    .as_ref()
                    .ok_or(DavWriteError::PreconditionRequired)?;
                if !expected.matches(&existing.etag) {
                    return Err(DavWriteError::PreconditionFailed.into());
                }
            }
            None if if_match.is_some() => {
                return Err(DavWriteError::PreconditionFailed.into());
            }
            None => {}
        }
        let stream_id = stable_stream_id(space_id, &resource_id);
        let dependencies: Vec<Uuid> = self
            .load_resource_head(&collection_id, &resource_id)
            .await?
            .into_iter()
            .collect();
        let parent_operation_id = dependencies.first().copied();
        let resource_kind = pim_kind(kind, &payload)?;
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
        self.queue_operation(envelope, updated_at_ms).await?;
        let outcome = if existing_resource.is_some() {
            UpsertOutcome::Updated
        } else {
            UpsertOutcome::Inserted
        };
        let response_etag = compute_etag(payload.as_bytes());
        self.store_operation_state(CachedOperationState {
            client_op_id,
            space_seq: 0,
            collection_id: collection_id.clone(),
            stream_id,
            logical_resource_id: resource_id.clone(),
            materialized_resource_id: resource_id.clone(),
            kind,
            payload: Some(payload),
            deleted: false,
            parent_operation_id,
            seed_projection_resource_id: None,
        })
        .await?;
        self.reconcile_pim_stream(collection_id.clone(), stream_id, resource_id, kind)
            .await?;
        drop(materialization_guard);
        let pushed = self.flush_outbox(space_id).await;
        let (cloud_space_seq, cloud_pushed) = match pushed {
            Ok(()) => {
                let acknowledged = self
                    .load_operation_state(collection_id.clone(), client_op_id)
                    .await?
                    .ok_or_else(|| anyhow!("flushed DAV operation state is missing"))?;
                anyhow::ensure!(
                    acknowledged.space_seq > 0,
                    "flushed DAV operation was not acknowledged"
                );
                (Some(acknowledged.space_seq), true)
            }
            Err(error) => {
                warn!(?error, %client_op_id, "DAV write retained in encrypted outbox");
                (None, false)
            }
        };
        Ok(PutResult {
            outcome,
            etag: response_etag,
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
        if_match: DavIfMatch,
    ) -> Result<bool> {
        let materialization_guard = self.materialization_lock.lock().await;
        let space_id = Uuid::parse_str(&collection_id).context("invalid security-space id")?;
        let (key_epoch, space_key) = self.current_space_key(space_id).await?;
        let identity = self
            .device_identity
            .as_ref()
            .ok_or_else(|| anyhow!("this DAV bridge device has not been approved"))?;
        let existing = self
            .get_resource(kind, collection_id.clone(), resource_id.clone())
            .await?
            .ok_or(DavWriteError::NotFound)?;
        if !if_match.matches(&existing.etag) {
            return Err(DavWriteError::PreconditionFailed.into());
        }
        let stream_id = stable_stream_id(space_id, &resource_id);
        let dependencies: Vec<Uuid> = self
            .load_resource_head(&collection_id, &resource_id)
            .await?
            .into_iter()
            .collect();
        let parent_operation_id = dependencies.first().copied();
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
        self.queue_operation(envelope, now_unix_ms()).await?;
        self.store_operation_state(CachedOperationState {
            client_op_id,
            space_seq: 0,
            collection_id: collection_id.clone(),
            stream_id,
            logical_resource_id: resource_id.clone(),
            materialized_resource_id: resource_id.clone(),
            kind,
            payload: None,
            deleted: true,
            parent_operation_id,
            seed_projection_resource_id: None,
        })
        .await?;
        self.reconcile_pim_stream(collection_id.clone(), stream_id, resource_id, kind)
            .await?;
        drop(materialization_guard);
        let _ = self.flush_outbox(space_id).await;
        Ok(true)
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

    async fn remove_queued_operation(&self, space_id: Uuid, operation_id: Uuid) -> Result<()> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.remove_queued_operation(space_id, operation_id))
            .await?
    }

    async fn flush_outbox(&self, space_id: Uuid) -> Result<()> {
        let _flush_guard = self.outbox_flush_lock.lock().await;
        let cache = self.cache.clone();
        let queued = tokio::task::spawn_blocking(move || cache.list_queued_operations()).await??;
        for envelope in queued
            .into_iter()
            .filter(|envelope| envelope.space_id == space_id)
        {
            let space_seq = self.cloud.append_operation(&envelope).await?;
            self.acknowledge_operation(
                envelope.space_id.to_string(),
                envelope.client_op_id,
                space_seq,
            )
            .await?;
            self.remove_queued_operation(envelope.space_id, envelope.client_op_id)
                .await?;
        }
        Ok(())
    }
}

#[cfg(feature = "local-bridge")]
fn pim_kind(kind: DavResourceKind, payload: &str) -> Result<PimResourceKind> {
    match kind {
        DavResourceKind::Contact => crate::pim::validate_dav_projection(true, payload),
        DavResourceKind::Calendar => crate::pim::validate_dav_projection(false, payload),
        DavResourceKind::Note => Err(anyhow!("DAV notes are unsupported")),
    }
}

fn dav_kind(kind: PimResourceKind) -> DavResourceKind {
    match kind {
        PimResourceKind::Contact => DavResourceKind::Contact,
        PimResourceKind::CalendarEvent | PimResourceKind::Task => DavResourceKind::Calendar,
    }
}

fn operation_resource_id(operation: &PimOperationV1) -> Uuid {
    match operation {
        PimOperationV1::Upsert(value) => value.resource_id,
        PimOperationV1::Delete(value) => value.resource_id,
    }
}

fn pim_kind_from_cached_state(state: &CachedOperationState) -> PimResourceKind {
    match state.kind {
        DavResourceKind::Contact => PimResourceKind::Contact,
        DavResourceKind::Calendar => state
            .payload
            .as_deref()
            .and_then(|payload| crate::pim::validate_dav_projection(false, payload).ok())
            .unwrap_or(PimResourceKind::CalendarEvent),
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

fn current_ical_utc() -> Result<String> {
    let format = time::macros::format_description!("[year][month][day]T[hour][minute][second]Z");
    time::OffsetDateTime::now_utc()
        .format(format)
        .map_err(Into::into)
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

fn validate_projection_resource_id(kind: PimResourceKind, value: &str) -> Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= 255
            && !value.contains('/')
            && !value.contains('\\')
            && !value.contains('\0'),
        "invalid PIM projection resource id"
    );
    let expected_suffix = match kind {
        PimResourceKind::Contact => ".vcf",
        PimResourceKind::CalendarEvent | PimResourceKind::Task => ".ics",
    };
    anyhow::ensure!(
        value.contains(expected_suffix),
        "PIM projection resource id does not match its kind"
    );
    Ok(())
}

fn dav_resource_id(upsert: &PimUpsertV1) -> String {
    match upsert.fields.get("dav_resource_id") {
        Some(PimValue::Text(value)) if !value.trim().is_empty() => value.clone(),
        _ => default_resource_id(upsert.resource_kind, upsert.resource_id),
    }
}

#[cfg(test)]
mod first_party_pim_tests {
    use super::*;

    #[test]
    fn cloud_url_is_a_secure_path_free_origin() {
        assert_eq!(
            normalize_cloud_base_url(" https://api.kamori.app/// ").expect("origin"),
            "https://api.kamori.app"
        );
        assert_eq!(
            normalize_cloud_base_url("http://127.0.0.1:3000/").expect("loopback"),
            "http://127.0.0.1:3000"
        );
        assert!(normalize_cloud_base_url("http://api.kamori.app").is_err());
        assert!(normalize_cloud_base_url("https://api.kamori.app/api").is_err());
        assert!(normalize_cloud_base_url("https://user@api.kamori.app").is_err());
    }

    #[test]
    fn partial_task_edit_preserves_existing_and_extension_properties() {
        let upsert = PimUpsertV1 {
            resource_kind: PimResourceKind::Task,
            resource_id: Uuid::from_u128(1),
            dependencies: Vec::new(),
            fields: BTreeMap::from([("completed".to_string(), PimValue::Boolean(true))]),
            raw_projection: Vec::new(),
        };
        let existing = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VTODO\r\nUID:00000000-0000-0000-0000-000000000001\r\nDTSTAMP:20260823T120000Z\r\nSUMMARY:Keep me\r\nX-KAMORI-UNKNOWN:value\r\nSTATUS:NEEDS-ACTION\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";
        let materialized = materialize_projection(&upsert, Some(existing)).expect("materialize");
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
        let queued = runner.state.cache.list_queued_operations().unwrap();
        assert_eq!(queued.len(), 2);
        let child = PimOperationV1::decode(&queued[1].open(&[9_u8; 32]).expect("open child"))
            .expect("decode child");
        let PimOperationV1::Delete(child) = child else {
            panic!("second queued operation must be the tombstone");
        };
        assert_eq!(child.dependencies, vec![queued[0].client_op_id]);

        let _ = std::fs::remove_file(db_path);
    }
}
