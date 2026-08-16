//! Origin/Referer validation helpers for browser cookie-mode requests.

use axum::http::{HeaderMap, header};
use url::Url;

use crate::{
    features::common::{ApiError, unauthorized},
    platform::config::Config,
};

fn normalize_origin(value: &str) -> Option<String> {
    let url = Url::parse(value.trim()).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let mut out = format!("{}://{}", url.scheme().to_ascii_lowercase(), host);
    if let Some(port) = url.port() {
        out.push(':');
        out.push_str(&port.to_string());
    }
    Some(out)
}

fn request_origin_from_headers(headers: &HeaderMap) -> Result<String, ApiError> {
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin
            .to_str()
            .map_err(|_| unauthorized("invalid origin header"))?;
        if origin.trim().eq_ignore_ascii_case("null") {
            return Err(unauthorized("invalid origin header"));
        }
        return normalize_origin(origin).ok_or_else(|| unauthorized("invalid origin header"));
    }

    if let Some(referer) = headers.get(header::REFERER) {
        let referer = referer
            .to_str()
            .map_err(|_| unauthorized("invalid referer header"))?;
        return normalize_origin(referer).ok_or_else(|| unauthorized("invalid referer header"));
    }

    Err(unauthorized("origin or referer is required"))
}

pub(crate) fn validate_cookie_request_origin(
    config: &Config,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    let request_origin = request_origin_from_headers(headers)?;

    let origin_allowed = config.cors_allow_origins.iter().any(|allowed_origin| {
        let trimmed = allowed_origin.trim();
        if trimmed == "*" {
            return true;
        }
        normalize_origin(trimmed)
            .map(|normalized| normalized == request_origin)
            .unwrap_or(false)
    });

    if !origin_allowed {
        return Err(unauthorized("origin mismatch"));
    }
    Ok(())
}
