//! Auth HTTP transport helpers.

mod cookies;
mod csrf;
mod origin;
mod refresh_transport;

pub(crate) use cookies::{
    clear_csrf_cookie, clear_refresh_cookie, read_csrf_cookie, read_refresh_cookie,
    set_csrf_cookie, set_refresh_cookie,
};
pub(crate) use csrf::{generate_csrf_token, validate_cookie_csrf};
pub(crate) use origin::validate_cookie_request_origin;
pub(crate) use refresh_transport::{
    RefreshTransport, client_metadata_from_headers, hash_refresh_token,
    refresh_transport_from_headers,
};

#[cfg(test)]
pub(crate) use csrf::CSRF_HEADER;
#[cfg(test)]
pub(crate) use refresh_transport::REFRESH_TRANSPORT_HEADER;

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{HeaderMap, HeaderValue, StatusCode, header},
        response::Response,
    };

    use crate::platform::test_support::test_config;

    use super::*;

    #[test]
    fn refresh_transport_defaults_to_body() {
        let headers = HeaderMap::new();
        let mode = refresh_transport_from_headers(&headers).expect("transport");
        assert_eq!(mode, RefreshTransport::Body);
    }

    #[test]
    fn refresh_transport_rejects_invalid_value() {
        let mut headers = HeaderMap::new();
        headers.insert(
            REFRESH_TRANSPORT_HEADER,
            HeaderValue::from_static("invalid-mode"),
        );
        let err = refresh_transport_from_headers(&headers).expect_err("must fail");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn cookie_origin_accepts_origin_header() {
        let config = test_config();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost:4173"),
        );

        validate_cookie_request_origin(&config, &headers).expect("origin allowed");
    }

    #[test]
    fn cookie_origin_accepts_referer_when_origin_missing() {
        let config = test_config();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::REFERER,
            HeaderValue::from_static("https://app.example.com/app"),
        );

        validate_cookie_request_origin(&config, &headers).expect("referer origin allowed");
    }

    #[test]
    fn cookie_origin_rejects_missing_origin_and_referer() {
        let config = test_config();
        let headers = HeaderMap::new();
        let err = validate_cookie_request_origin(&config, &headers).expect_err("must fail");
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert_eq!(err.1.0.message, "origin or referer is required");
    }

    #[test]
    fn cookie_origin_rejects_mismatch() {
        let config = test_config();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.example"),
        );
        let err = validate_cookie_request_origin(&config, &headers).expect_err("must fail");
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert_eq!(err.1.0.message, "origin mismatch");
    }

    #[test]
    fn cookie_csrf_requires_matching_header_and_cookie() {
        let config = test_config();
        let mut headers = HeaderMap::new();
        headers.insert(CSRF_HEADER, HeaderValue::from_static("abc123"));
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("{}=abc123", config.web_csrf_cookie_name))
                .expect("cookie header"),
        );

        validate_cookie_csrf(&config, &headers).expect("csrf ok");
    }

    #[test]
    fn cookie_csrf_rejects_mismatch() {
        let config = test_config();
        let mut headers = HeaderMap::new();
        headers.insert(CSRF_HEADER, HeaderValue::from_static("abc123"));
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("{}=zzz", config.web_csrf_cookie_name))
                .expect("cookie header"),
        );

        let err = validate_cookie_csrf(&config, &headers).expect_err("must fail");
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert_eq!(err.1.0.message, "csrf token mismatch");
    }

    #[test]
    fn cookie_helpers_set_and_clear_refresh_and_csrf_cookies() {
        let config = test_config();
        let mut response = Response::new(Body::empty());

        set_refresh_cookie(&config, &mut response, "refresh-token").expect("set refresh");
        set_csrf_cookie(&config, &mut response, "csrf-token").expect("set csrf");
        clear_refresh_cookie(&config, &mut response).expect("clear refresh");
        clear_csrf_cookie(&config, &mut response).expect("clear csrf");

        let cookies: Vec<String> = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok().map(str::to_owned))
            .collect();

        assert!(
            cookies
                .iter()
                .any(|cookie: &String| cookie.starts_with("__Host-kamori_rt=refresh-token;"))
        );
        assert!(
            cookies
                .iter()
                .any(|cookie: &String| cookie.starts_with("__Host-kamori_csrf=csrf-token;"))
        );
        assert!(cookies.iter().all(|cookie| cookie.contains("; HttpOnly;")));
        assert!(
            cookies
                .iter()
                .any(|cookie: &String| cookie.starts_with("__Host-kamori_rt=;"))
        );
        assert!(
            cookies
                .iter()
                .any(|cookie: &String| cookie.starts_with("__Host-kamori_csrf=;"))
        );
    }
}
