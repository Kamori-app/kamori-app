//! CSRF helpers for cookie-mode refresh/logout endpoints.

use axum::http::HeaderMap;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngExt;

use crate::{
    features::common::{ApiError, unauthorized},
    platform::config::Config,
};

use super::cookies::read_csrf_cookie;

pub(crate) const CSRF_HEADER: &str = "x-kamori-csrf-token";

pub(crate) fn validate_cookie_csrf(config: &Config, headers: &HeaderMap) -> Result<(), ApiError> {
    let header_token = headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| unauthorized("csrf token is required"))?;
    let cookie_token =
        read_csrf_cookie(config, headers).ok_or_else(|| unauthorized("csrf token is required"))?;
    if header_token != cookie_token {
        return Err(unauthorized("csrf token mismatch"));
    }
    Ok(())
}

pub(crate) fn generate_csrf_token() -> String {
    let mut raw = [0u8; 32];
    let mut rng = rand::rng();
    rng.fill(&mut raw);
    URL_SAFE_NO_PAD.encode(raw)
}
