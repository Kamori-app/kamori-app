//! Device API models.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DevicePlatform {
    Web,
    Desktop,
    Android,
    Ios,
}

impl DevicePlatform {
    pub(crate) const fn as_db_value(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Desktop => "desktop",
            Self::Android => "android",
            Self::Ios => "ios",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RegisterDeviceRequest {
    pub enrollment_token: String,
    pub device_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub signing_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub hpke_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub encrypted_name: Vec<u8>,
    pub platform: DevicePlatform,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeviceSummary {
    pub device_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub signing_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub hpke_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub encrypted_name: Vec<u8>,
    pub platform: DevicePlatform,
    pub created_at_unix_ms: i64,
    pub last_seen_at_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RegisterDeviceResponse {
    pub device: DeviceSummary,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ListDevicesResponse {
    pub devices: Vec<DeviceSummary>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RevokeDeviceResponse {
    pub revoked: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RevokeDeviceRequest {
    pub reauth_token: String,
}
