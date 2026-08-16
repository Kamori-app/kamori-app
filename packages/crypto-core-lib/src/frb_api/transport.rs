use reqwest::{Client, StatusCode};
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

use super::types::{MobileRefreshRequest, MobileRefreshResponse};

pub(super) const MSGPACK_CONTENT_TYPE: &str = "application/msgpack";
const MOBILE_HTTP_CONNECT_TIMEOUT_SECS: u64 = 5;
const MOBILE_HTTP_REQUEST_TIMEOUT_SECS: u64 = 20;

pub(super) fn mobile_http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(MOBILE_HTTP_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(MOBILE_HTTP_REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|error| format!("failed to build HTTP client: {error}"))
}

pub(super) fn endpoint(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

pub(super) fn encode_msgpack<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    rmp_serde::to_vec_named(value).map_err(|error| error.to_string())
}

pub(super) async fn decode_msgpack<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, String> {
    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    rmp_serde::from_slice(&bytes).map_err(|error| error.to_string())
}

pub(super) async fn refresh_mobile_tokens(
    cloud_base_url: &str,
    refresh_token: &str,
) -> Result<(String, String), String> {
    let http = mobile_http_client()?;
    let body = encode_msgpack(&MobileRefreshRequest {
        refresh_token: refresh_token.to_string(),
    })?;
    let response = http
        .post(endpoint(cloud_base_url, "/auth/refresh"))
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(body)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let refreshed: MobileRefreshResponse = decode_msgpack(response).await?;
    Ok((refreshed.access_token, refreshed.refresh_token))
}

pub(super) async fn post_msgpack_with_auth_refresh<T>(
    cloud_base_url: &str,
    path: &str,
    body: Vec<u8>,
    access_token: &str,
    refresh_token: Option<&str>,
) -> Result<(T, Option<(String, String)>), String>
where
    T: DeserializeOwned,
{
    let http = mobile_http_client()?;
    let response = http
        .post(endpoint(cloud_base_url, path))
        .bearer_auth(access_token)
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(body.clone())
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if response.status() == StatusCode::UNAUTHORIZED {
        let Some(refresh_token) = refresh_token else {
            return Err("request unauthorized and refresh token is unavailable".to_string());
        };
        let (new_access_token, new_refresh_token) =
            refresh_mobile_tokens(cloud_base_url, refresh_token).await?;
        let retried = http
            .post(endpoint(cloud_base_url, path))
            .bearer_auth(&new_access_token)
            .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
            .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
            .body(body)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        let payload: T = decode_msgpack(retried).await?;
        return Ok((payload, Some((new_access_token, new_refresh_token))));
    }

    let response = response
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let payload: T = decode_msgpack(response).await?;
    Ok((payload, None))
}

pub(super) async fn get_msgpack_with_auth_refresh<T>(
    cloud_base_url: &str,
    path: &str,
    access_token: &str,
    refresh_token: Option<&str>,
) -> Result<(T, Option<(String, String)>), String>
where
    T: DeserializeOwned,
{
    let http = mobile_http_client()?;
    let response = http
        .get(endpoint(cloud_base_url, path))
        .bearer_auth(access_token)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if response.status() == StatusCode::UNAUTHORIZED {
        let Some(refresh_token) = refresh_token else {
            return Err("request unauthorized and refresh token is unavailable".to_string());
        };
        let (new_access_token, new_refresh_token) =
            refresh_mobile_tokens(cloud_base_url, refresh_token).await?;
        let retried = http
            .get(endpoint(cloud_base_url, path))
            .bearer_auth(&new_access_token)
            .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        return Ok((
            decode_msgpack(retried).await?,
            Some((new_access_token, new_refresh_token)),
        ));
    }

    Ok((
        decode_msgpack(
            response
                .error_for_status()
                .map_err(|error| error.to_string())?,
        )
        .await?,
        None,
    ))
}

pub(super) async fn delete_msgpack_with_auth_refresh<T>(
    cloud_base_url: &str,
    path: &str,
    access_token: &str,
    refresh_token: Option<&str>,
) -> Result<(T, Option<(String, String)>), String>
where
    T: DeserializeOwned,
{
    let http = mobile_http_client()?;
    let response = http
        .delete(endpoint(cloud_base_url, path))
        .bearer_auth(access_token)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if response.status() == StatusCode::UNAUTHORIZED {
        let Some(refresh_token) = refresh_token else {
            return Err("request unauthorized and refresh token is unavailable".to_string());
        };
        let (new_access_token, new_refresh_token) =
            refresh_mobile_tokens(cloud_base_url, refresh_token).await?;
        let retried = http
            .delete(endpoint(cloud_base_url, path))
            .bearer_auth(&new_access_token)
            .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        return Ok((
            decode_msgpack(retried).await?,
            Some((new_access_token, new_refresh_token)),
        ));
    }

    Ok((
        decode_msgpack(
            response
                .error_for_status()
                .map_err(|error| error.to_string())?,
        )
        .await?,
        None,
    ))
}
