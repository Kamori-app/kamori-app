//! Authentication helpers for bearer tokens and TOTP verification.
use axum::http::HeaderMap;
use hmac::{KeyInit, Mac, SimpleHmac};
use sha1::Sha1;
use time::OffsetDateTime;

/// Configuration for TOTP verification.
#[derive(Clone, Copy, Debug)]
pub struct TotpConfig {
    /// Time step in seconds.
    pub step_seconds: i64,
    /// Number of digits expected in the code.
    pub digits: u32,
    /// Allowed skew in steps around the current time.
    pub allowed_skew_steps: i64,
}

impl Default for TotpConfig {
    fn default() -> Self {
        Self {
            step_seconds: 30,
            digits: 6,
            allowed_skew_steps: 1,
        }
    }
}

/// Errors produced by TOTP validation.
#[derive(Debug, thiserror::Error)]
pub enum TotpError {
    #[error("invalid base32 secret")]
    InvalidBase32,
    #[error("invalid totp code")]
    InvalidCode,
}

/// Parses an Authorization header value and returns the bearer token, if any.
pub fn parse_bearer(header_value: &str) -> Option<&str> {
    let mut parts = header_value.split_whitespace();
    let scheme = parts.next()?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = parts.next()?;
    if token.is_empty() { None } else { Some(token) }
}

/// Extracts a bearer token from HTTP headers.
pub fn bearer_from_headers(headers: &HeaderMap) -> Option<String> {
    let header = headers.get(axum::http::header::AUTHORIZATION)?;
    let header_str = header.to_str().ok()?;
    parse_bearer(header_str).map(|s| s.to_string())
}

/// Verifies a TOTP code (RFC 6238) using HMAC-SHA1 and base32 secret.
pub fn verify_totp(
    secret_b32: &str,
    code: &str,
    now: OffsetDateTime,
    cfg: TotpConfig,
) -> Result<bool, TotpError> {
    let secret = base32_decode(secret_b32).ok_or(TotpError::InvalidBase32)?;
    if code.len() as u32 != cfg.digits || !code.chars().all(|c| c.is_ascii_digit()) {
        return Err(TotpError::InvalidCode);
    }

    let timestep = now.unix_timestamp() / cfg.step_seconds;
    for offset in -cfg.allowed_skew_steps..=cfg.allowed_skew_steps {
        let counter = timestep + offset;
        if counter < 0 {
            continue;
        }
        let generated = hotp(&secret, counter as u64, cfg.digits);
        if generated == code {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Computes an HOTP value for a given counter.
fn hotp(secret: &[u8], counter: u64, digits: u32) -> String {
    let mut mac =
        SimpleHmac::<Sha1>::new_from_slice(secret).expect("hmac can take key of any size");
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();

    let offset = (result[result.len() - 1] & 0x0f) as usize;
    let slice = &result[offset..offset + 4];

    let mut truncated = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]);
    truncated &= 0x7fff_ffff;

    let modulo = 10u32.pow(digits);
    let code = truncated % modulo;
    format!("{:0width$}", code, width = digits as usize)
}

#[cfg(test)]
pub(crate) fn generate_totp_for_test(secret_b32: &str, now: OffsetDateTime) -> String {
    let config = TotpConfig::default();
    let secret = base32_decode(secret_b32).expect("test TOTP secret");
    hotp(
        &secret,
        (now.unix_timestamp() / config.step_seconds) as u64,
        config.digits,
    )
}

/// Decodes a base32 string into bytes.
fn base32_decode(input: &str) -> Option<Vec<u8>> {
    let mut buffer: u32 = 0;
    let mut bits_left = 0u8;
    let mut out = Vec::new();

    for ch in input.chars() {
        if ch == '=' || ch.is_whitespace() || ch == '-' {
            continue;
        }
        let val = base32_value(ch)?;
        buffer = (buffer << 5) | val as u32;
        bits_left += 5;

        while bits_left >= 8 {
            bits_left -= 8;
            let byte = ((buffer >> bits_left) & 0xff) as u8;
            out.push(byte);
        }
    }

    Some(out)
}

/// Converts a base32 character into its 5-bit value.
fn base32_value(ch: char) -> Option<u8> {
    match ch {
        'A'..='Z' => Some((ch as u8) - b'A'),
        'a'..='z' => Some((ch as u8) - b'a'),
        '2'..='7' => Some((ch as u8) - b'2' + 26),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, header::AUTHORIZATION};

    #[test]
    fn parse_bearer_accepts_valid_token() {
        assert_eq!(parse_bearer("Bearer token123"), Some("token123"));
        assert_eq!(parse_bearer("bearer token123"), Some("token123"));
        assert_eq!(parse_bearer("Basic token123"), None);
        assert_eq!(parse_bearer("Bearer"), None);
    }

    #[test]
    fn bearer_from_headers_reads_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "Bearer abc".parse().unwrap());
        assert_eq!(bearer_from_headers(&headers), Some("abc".to_string()));
    }

    #[test]
    fn verify_totp_matches_rfc_vector() {
        let cfg = TotpConfig {
            step_seconds: 30,
            digits: 8,
            allowed_skew_steps: 0,
        };
        let now = OffsetDateTime::from_unix_timestamp(59).unwrap();
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let ok = verify_totp(secret, "94287082", now, cfg).unwrap();
        assert!(ok);
    }

    #[test]
    fn verify_totp_rejects_invalid_secret() {
        let now = OffsetDateTime::from_unix_timestamp(59).unwrap();
        let err = verify_totp("!@#", "123456", now, TotpConfig::default()).unwrap_err();
        assert!(matches!(err, TotpError::InvalidBase32));
    }
}
