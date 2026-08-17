//! Refresh transport negotiation and token material helpers.

use axum::http::HeaderMap;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

use crate::features::common::{ApiError, bad_request, unauthenticated};

pub(crate) struct ClientMetadata {
    pub(crate) user_agent: Option<String>,
    pub(crate) ip_address: Option<String>,
}

pub(crate) const REFRESH_TRANSPORT_HEADER: &str = "x-kamori-refresh-transport";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefreshTransport {
    Body,
    Cookie,
}

pub(crate) fn refresh_transport_from_headers(
    headers: &HeaderMap,
) -> Result<RefreshTransport, ApiError> {
    let Some(value) = headers.get(REFRESH_TRANSPORT_HEADER) else {
        return Ok(RefreshTransport::Body);
    };
    let value = value
        .to_str()
        .map_err(|_| bad_request("invalid refresh transport header"))?;
    match value.trim().to_ascii_lowercase().as_str() {
        "body" => Ok(RefreshTransport::Body),
        "cookie" => Ok(RefreshTransport::Cookie),
        _ => Err(bad_request(
            "invalid refresh transport header (expected `body` or `cookie`)",
        )),
    }
}

fn decode_refresh_token_material(refresh_token: &str) -> Result<Vec<u8>, ApiError> {
    let raw = URL_SAFE_NO_PAD
        .decode(refresh_token.as_bytes())
        .map_err(|_| unauthenticated("invalid refresh token"))?;
    if raw.len() < 32 {
        return Err(unauthenticated("invalid refresh token"));
    }
    Ok(raw)
}

pub(crate) fn hash_refresh_token(refresh_token: &str) -> Result<Vec<u8>, ApiError> {
    let raw = decode_refresh_token_material(refresh_token)?;
    Ok(Sha256::digest(raw).to_vec())
}

pub(crate) fn client_metadata_from_headers(headers: &HeaderMap) -> ClientMetadata {
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let ip_address = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.split(',').next())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    ClientMetadata {
        user_agent,
        ip_address,
    }
}
