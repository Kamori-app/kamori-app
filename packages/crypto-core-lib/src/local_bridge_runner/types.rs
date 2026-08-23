use serde::{Deserialize, Serialize};
use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
};
use uuid::Uuid;

/// Resource kind supported by the local DAV cache.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DavResourceKind {
    /// CardDAV contact payloads (`.vcf`).
    Contact,
    /// CalDAV calendar payloads (`.ics`).
    Calendar,
    /// Plain-text note payloads.
    Note,
}

impl DavResourceKind {
    /// Parses kind from URL scope segment.
    #[cfg(feature = "local-bridge")]
    pub(crate) fn from_route_segment(segment: &str) -> Option<Self> {
        match segment {
            "carddav" | "contacts" | "contact" => Some(Self::Contact),
            "caldav" | "calendars" | "calendar" => Some(Self::Calendar),
            "notes" | "note" => Some(Self::Note),
            _ => None,
        }
    }

    /// Returns DAV path prefix used by local server routes.
    pub(crate) fn route_prefix(self) -> &'static str {
        match self {
            Self::Contact => "carddav",
            Self::Calendar => "caldav",
            Self::Note => "notes",
        }
    }

    /// Returns SQLite table name for the resource kind.
    pub(crate) fn table_name(self) -> &'static str {
        match self {
            Self::Contact => "contacts",
            Self::Calendar => "calendars",
            Self::Note => "notes",
        }
    }

    /// Returns SQLite payload column name for the resource kind.
    pub(crate) fn payload_column(self) -> &'static str {
        match self {
            Self::Contact => "vcard",
            Self::Calendar => "ical",
            Self::Note => "note_text",
        }
    }

    /// Returns HTTP content type for GET responses.
    #[cfg(feature = "local-bridge")]
    pub(crate) fn content_type(self) -> &'static str {
        match self {
            Self::Contact => "text/vcard; charset=utf-8",
            Self::Calendar => "text/calendar; charset=utf-8",
            Self::Note => "text/markdown; charset=utf-8",
        }
    }
}

/// Configuration for [`super::LocalBridgeRunner`].
#[derive(Clone, Debug)]
pub struct LocalBridgeConfig {
    /// Path to the local SQLite cache file.
    pub sqlite_path: PathBuf,
    /// SQLCipher key for encrypting/decrypting the local cache file.
    pub sqlite_key: Option<String>,
    /// Local bind address for the embedded HTTP server.
    pub bind_addr: SocketAddr,
    /// Base URL of the cloud server.
    pub cloud_base_url: String,
    /// Bearer access token used for cloud API calls.
    pub access_token: String,
    /// Opaque refresh token used to rotate expired access tokens.
    pub refresh_token: Option<String>,
    /// Approved device identity used to sign locally-created operations.
    pub device_identity: Option<LocalDeviceIdentity>,
    /// Dedicated random Basic Auth username for localhost DAV clients.
    pub dav_username: Option<String>,
    /// Dedicated random Basic Auth password for localhost DAV clients.
    pub dav_password: Option<String>,
}

impl LocalBridgeConfig {
    /// Builds a new configuration with default bind address `127.0.0.1:8181`.
    pub fn new(
        sqlite_path: impl Into<PathBuf>,
        cloud_base_url: impl Into<String>,
        access_token: impl Into<String>,
    ) -> Self {
        Self {
            sqlite_path: sqlite_path.into(),
            sqlite_key: None,
            bind_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 8181)),
            cloud_base_url: cloud_base_url.into(),
            access_token: access_token.into(),
            refresh_token: None,
            device_identity: None,
            dav_username: None,
            dav_password: None,
        }
    }

    /// Overrides the default local bind address.
    pub fn with_bind_addr(mut self, bind_addr: SocketAddr) -> Self {
        self.bind_addr = bind_addr;
        self
    }

    /// Sets SQLCipher key used for local cache encryption at rest.
    pub fn with_sqlite_key(mut self, sqlite_key: impl Into<String>) -> Self {
        self.sqlite_key = Some(sqlite_key.into());
        self
    }

    /// Sets refresh token used for access token rotation.
    pub fn with_refresh_token(mut self, refresh_token: impl Into<String>) -> Self {
        self.refresh_token = Some(refresh_token.into());
        self
    }

    /// Supplies the approved device signing identity for offline writes.
    pub fn with_device_identity(mut self, identity: LocalDeviceIdentity) -> Self {
        self.device_identity = Some(identity);
        self
    }

    /// Configures dedicated localhost DAV credentials (never the account password).
    pub fn with_dav_credentials(
        mut self,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.dav_username = Some(username.into());
        self.dav_password = Some(password.into());
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalDeviceIdentity {
    pub device_id: Uuid,
    pub signing_private_key: [u8; 32],
}

/// A single decrypted resource row stored in the local cache.
#[derive(Clone, Debug)]
pub struct LocalResource {
    /// DAV resource kind.
    pub kind: DavResourceKind,
    /// Collection identifier.
    pub collection_id: String,
    /// Resource identifier.
    pub resource_id: String,
    /// Resource contents in clear text.
    pub payload: String,
    /// SHA-256-based ETag for DAV responses.
    pub etag: String,
    /// Last update timestamp used by LWW conflict handling.
    pub updated_at_ms: i64,
}

/// One explicit materialized branch of a logical PIM stream.
///
/// First-party clients must use `projection_resource_id` together with
/// `head_operation_id` when mutating an existing item. This keeps conflict
/// branches distinct and provides optimistic concurrency without parsing DAV
/// filenames.
#[derive(Clone, Debug)]
pub struct MaterializedPimBranch {
    pub space_id: Uuid,
    pub logical_resource_id: Uuid,
    pub projection_resource_id: String,
    pub head_operation_id: Uuid,
    pub kind: DavResourceKind,
    pub payload: Option<String>,
    pub deleted: bool,
    pub conflict: bool,
}

/// A durable local DAV collection change used by RFC 6578 sync reports.
#[cfg(feature = "local-bridge")]
#[derive(Clone, Debug)]
pub(crate) struct DavChange {
    pub(crate) revision: u64,
    pub(crate) resource_id: String,
    pub(crate) deleted: bool,
}

/// Result of LWW upsert into the local cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpsertOutcome {
    /// New row inserted.
    Inserted,
    /// Existing row updated.
    Updated,
    /// Incoming row ignored because it is stale.
    IgnoredStale,
}
