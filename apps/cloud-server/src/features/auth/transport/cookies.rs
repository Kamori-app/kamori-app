//! Cookie read/write helpers for refresh and CSRF tokens.

use axum::{
    http::{HeaderMap, HeaderValue, header},
    response::Response,
};

use crate::{
    features::common::{ApiError, internal_error},
    platform::config::Config,
};

pub(crate) fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for chunk in cookie_header.split(';') {
        let mut parts = chunk.trim().splitn(2, '=');
        let key = parts.next()?.trim();
        if key != name {
            continue;
        }
        let value = parts.next().unwrap_or_default().trim();
        if value.is_empty() {
            return None;
        }
        return Some(value.to_string());
    }
    None
}

pub(crate) fn read_refresh_cookie(config: &Config, headers: &HeaderMap) -> Option<String> {
    read_cookie(headers, &config.web_refresh_cookie_name)
}

pub(crate) fn read_csrf_cookie(config: &Config, headers: &HeaderMap) -> Option<String> {
    read_cookie(headers, &config.web_csrf_cookie_name)
}

fn refresh_cookie_same_site(config: &Config) -> &'static str {
    if config
        .web_refresh_cookie_same_site
        .eq_ignore_ascii_case("strict")
    {
        "Strict"
    } else if config
        .web_refresh_cookie_same_site
        .eq_ignore_ascii_case("none")
    {
        "None"
    } else {
        "Lax"
    }
}

pub(crate) fn set_refresh_cookie(
    config: &Config,
    response: &mut Response,
    refresh_token: &str,
) -> Result<(), ApiError> {
    let mut cookie = format!(
        "{}={}; Path={}; Max-Age={}; HttpOnly; SameSite={}",
        config.web_refresh_cookie_name,
        refresh_token,
        config.web_refresh_cookie_path,
        config.refresh_token_ttl_seconds,
        refresh_cookie_same_site(config),
    );
    if let Some(domain) = &config.web_refresh_cookie_domain {
        cookie.push_str("; Domain=");
        cookie.push_str(domain);
    }
    if config.web_refresh_cookie_secure {
        cookie.push_str("; Secure");
    }
    let value = HeaderValue::from_str(&cookie).map_err(internal_error)?;
    response.headers_mut().append(header::SET_COOKIE, value);
    Ok(())
}

pub(crate) fn set_csrf_cookie(
    config: &Config,
    response: &mut Response,
    csrf_token: &str,
) -> Result<(), ApiError> {
    let mut cookie = format!(
        "{}={}; Path={}; Max-Age={}; SameSite={}",
        config.web_csrf_cookie_name,
        csrf_token,
        config.web_refresh_cookie_path,
        config.refresh_token_ttl_seconds,
        refresh_cookie_same_site(config),
    );
    if let Some(domain) = &config.web_refresh_cookie_domain {
        cookie.push_str("; Domain=");
        cookie.push_str(domain);
    }
    if config.web_refresh_cookie_secure {
        cookie.push_str("; Secure");
    }
    let value = HeaderValue::from_str(&cookie).map_err(internal_error)?;
    response.headers_mut().append(header::SET_COOKIE, value);
    Ok(())
}

pub(crate) fn clear_refresh_cookie(
    config: &Config,
    response: &mut Response,
) -> Result<(), ApiError> {
    let mut cookie = format!(
        "{}=; Path={}; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; HttpOnly; SameSite={}",
        config.web_refresh_cookie_name,
        config.web_refresh_cookie_path,
        refresh_cookie_same_site(config),
    );
    if let Some(domain) = &config.web_refresh_cookie_domain {
        cookie.push_str("; Domain=");
        cookie.push_str(domain);
    }
    if config.web_refresh_cookie_secure {
        cookie.push_str("; Secure");
    }
    let value = HeaderValue::from_str(&cookie).map_err(internal_error)?;
    response.headers_mut().append(header::SET_COOKIE, value);
    Ok(())
}

pub(crate) fn clear_csrf_cookie(config: &Config, response: &mut Response) -> Result<(), ApiError> {
    let mut cookie = format!(
        "{}=; Path={}; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; SameSite={}",
        config.web_csrf_cookie_name,
        config.web_refresh_cookie_path,
        refresh_cookie_same_site(config),
    );
    if let Some(domain) = &config.web_refresh_cookie_domain {
        cookie.push_str("; Domain=");
        cookie.push_str(domain);
    }
    if config.web_refresh_cookie_secure {
        cookie.push_str("; Secure");
    }
    let value = HeaderValue::from_str(&cookie).map_err(internal_error)?;
    response.headers_mut().append(header::SET_COOKIE, value);
    Ok(())
}
