//! Origin/Referer validation helpers for browser cookie-mode requests.

use axum::http::{HeaderMap, header};
use url::Url;

use crate::{
    features::common::{ApiError, unauthorized},
    platform::config::Config,
};

fn normalize_origin(value: &str) -> Option<String> {
    let url = Url::parse(value.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    Some(url.origin().ascii_serialization())
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

    let origin_allowed = config.web_cookie_origins.iter().any(|allowed_origin| {
        let trimmed = allowed_origin.trim();
        normalize_origin(trimmed)
            .map(|normalized| normalized == request_origin)
            .unwrap_or(false)
    });

    if !origin_allowed {
        return Err(unauthorized("origin mismatch"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_normalization_accepts_referer_paths_but_rejects_opaque_schemes() {
        assert_eq!(
            normalize_origin("https://App.Kamori.App/settings?tab=security"),
            Some("https://app.kamori.app".to_string())
        );
        assert_eq!(
            normalize_origin("http://[::1]:4173/app"),
            Some("http://[::1]:4173".to_string())
        );
        assert!(normalize_origin("javascript://app.kamori.app").is_none());
        assert!(normalize_origin("https://user@app.kamori.app").is_none());
    }

    #[test]
    fn cookie_origin_wildcard_is_not_supported() {
        let mut config = crate::platform::test_support::test_config();
        config.web_cookie_origins = vec!["*".to_string()];
        let headers = HeaderMap::from_iter([(
            header::ORIGIN,
            "https://attacker.example".parse().expect("origin header"),
        )]);
        assert!(validate_cookie_request_origin(&config, &headers).is_err());
    }
}
