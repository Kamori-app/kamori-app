//! MessagePack client for the signed security-space operation API.

use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::operation_envelope::OperationEnvelopeV1;

const MSGPACK_CONTENT_TYPE: &str = "application/msgpack";
const HTTP_CONNECT_TIMEOUT_SECS: u64 = 5;
const HTTP_REQUEST_TIMEOUT_SECS: u64 = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CloudStoredOperation {
    pub(crate) space_seq: u64,
    pub(crate) received_at_unix_ms: i64,
    pub(crate) envelope: OperationEnvelopeV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CloudSpaceDevice {
    pub(crate) device_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub(crate) signing_public_key: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct CloudOperationsPage {
    pub(crate) operations: Vec<CloudStoredOperation>,
    pub(crate) next_cursor: u64,
}

#[derive(Clone)]
pub(crate) struct CloudSyncClient {
    base_url: String,
    tokens: Arc<RwLock<CloudAuthTokens>>,
    refresh_lock: Arc<Mutex<()>>,
    http: Client,
}

impl CloudSyncClient {
    pub(crate) fn new(
        base_url: String,
        access_token: String,
        refresh_token: Option<String>,
    ) -> Result<Self> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(HTTP_CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(HTTP_REQUEST_TIMEOUT_SECS))
            .build()
            .context("build cloud sync HTTP client")?;
        Ok(Self {
            base_url,
            tokens: Arc::new(RwLock::new(CloudAuthTokens {
                access_token,
                refresh_token,
            })),
            refresh_lock: Arc::new(Mutex::new(())),
            http,
        })
    }

    fn endpoint(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    pub(crate) async fn fetch_operations(
        &self,
        space_id: Uuid,
        since: u64,
    ) -> Result<CloudOperationsPage> {
        let mut url = Url::parse(&self.endpoint("/operations")).context("build operations URL")?;
        url.query_pairs_mut()
            .append_pair("space_id", &space_id.to_string())
            .append_pair("since", &since.to_string())
            .append_pair("limit", "1000");
        let response = self
            .send_with_access_retry(
                move |access_token| {
                    self.http
                        .get(url.clone())
                        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
                        .bearer_auth(access_token)
                },
                "cloud operations fetch",
            )
            .await?;
        let response: CloudListOperationsResponse = decode_msgpack(response).await?;
        Ok(CloudOperationsPage {
            operations: response.operations,
            next_cursor: response.next_cursor,
        })
    }

    pub(crate) async fn fetch_space_devices(
        &self,
        space_id: Uuid,
    ) -> Result<Vec<CloudSpaceDevice>> {
        let url = self.endpoint(&format!("/spaces/{space_id}/devices"));
        let response = self
            .send_with_access_retry(
                move |access_token| {
                    self.http
                        .get(&url)
                        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
                        .bearer_auth(access_token)
                },
                "cloud space device directory fetch",
            )
            .await?;
        Ok(decode_msgpack::<CloudListSpaceDevicesResponse>(response)
            .await?
            .devices)
    }

    pub(crate) async fn append_operation(&self, envelope: &OperationEnvelopeV1) -> Result<u64> {
        let url = self.endpoint("/operations");
        let body = Bytes::from(encode_msgpack(&CloudAppendOperationRequest { envelope })?);
        let response = self
            .send_with_access_retry(
                move |access_token| {
                    self.http
                        .post(&url)
                        .bearer_auth(access_token)
                        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
                        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
                        .body(body.clone())
                },
                "cloud operation append",
            )
            .await?;
        let payload: CloudAppendOperationResponse = decode_msgpack(response).await?;
        if !payload.accepted {
            return Err(anyhow!("cloud rejected operation"));
        }
        Ok(payload.space_seq)
    }

    pub(crate) async fn current_refresh_token(&self) -> Option<String> {
        self.tokens.read().await.refresh_token.clone()
    }

    async fn send_with_access_retry<F>(
        &self,
        build_request: F,
        operation: &str,
    ) -> Result<reqwest::Response>
    where
        F: Fn(&str) -> reqwest::RequestBuilder,
    {
        let access_token = self.tokens.read().await.access_token.clone();
        let response = build_request(&access_token)
            .send()
            .await
            .with_context(|| format!("{operation}: request failed"))?;
        if response.status() != StatusCode::UNAUTHORIZED {
            return response
                .error_for_status()
                .with_context(|| format!("{operation}: non-success status"));
        }
        if !self.try_refresh_tokens().await? {
            return Err(anyhow!("{operation}: unauthorized and refresh failed"));
        }
        let access_token = self.tokens.read().await.access_token.clone();
        build_request(&access_token)
            .send()
            .await
            .with_context(|| format!("{operation}: retry request failed"))?
            .error_for_status()
            .with_context(|| format!("{operation}: retry returned non-success status"))
    }

    async fn try_refresh_tokens(&self) -> Result<bool> {
        let _guard = self.refresh_lock.lock().await;
        let Some(refresh_token) = self.tokens.read().await.refresh_token.clone() else {
            return Ok(false);
        };
        let rotation_request_id = refresh_rotation_request_id(&refresh_token);
        let body = encode_msgpack(&CloudRefreshRequest {
            refresh_token: Some(refresh_token),
            rotation_request_id,
        })?;
        let response = self
            .http
            .post(self.endpoint("/auth/refresh"))
            .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
            .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
            .body(body)
            .send()
            .await
            .context("request auth refresh failed")?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Ok(false);
        }
        let payload: CloudRefreshResponse = decode_msgpack(response.error_for_status()?).await?;
        let Some(refresh_token) = payload.refresh_token else {
            return Err(anyhow!("body refresh transport returned no refresh token"));
        };
        let mut tokens = self.tokens.write().await;
        tokens.access_token = payload.access_token;
        tokens.refresh_token = Some(refresh_token);
        Ok(true)
    }
}

#[derive(Debug, Clone)]
struct CloudAuthTokens {
    access_token: String,
    refresh_token: Option<String>,
}

#[derive(Serialize)]
struct CloudAppendOperationRequest<'a> {
    envelope: &'a OperationEnvelopeV1,
}

#[derive(Deserialize)]
struct CloudAppendOperationResponse {
    accepted: bool,
    #[allow(dead_code)]
    duplicate: bool,
    space_seq: u64,
}

#[derive(Deserialize)]
struct CloudListOperationsResponse {
    operations: Vec<CloudStoredOperation>,
    next_cursor: u64,
}

#[derive(Deserialize)]
struct CloudListSpaceDevicesResponse {
    devices: Vec<CloudSpaceDevice>,
}

#[derive(Serialize)]
struct CloudRefreshRequest {
    refresh_token: Option<String>,
    rotation_request_id: Uuid,
}

fn refresh_rotation_request_id(refresh_token: &str) -> Uuid {
    let digest = Sha256::digest(
        [
            b"kamori.refresh-request.v1\0".as_slice(),
            refresh_token.as_bytes(),
        ]
        .concat(),
    );
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[derive(Deserialize)]
struct CloudRefreshResponse {
    access_token: String,
    refresh_token: Option<String>,
}

fn encode_msgpack<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    value
        .serialize(
            &mut rmp_serde::Serializer::new(&mut bytes)
                .with_struct_map()
                .with_human_readable(),
        )
        .context("serialize msgpack payload")?;
    Ok(bytes)
}

async fn decode_msgpack<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    let bytes = response.bytes().await.context("read msgpack body bytes")?;
    let mut deserializer = rmp_serde::Deserializer::new(bytes.as_ref()).with_human_readable();
    T::deserialize(&mut deserializer).context("decode msgpack payload")
}
