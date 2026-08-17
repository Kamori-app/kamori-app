//! Shared auth-service helpers: token hashing, code generation and normalization.

use rand::RngExt;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use url::Url;

use crate::{
    features::common::{ApiError, bad_request, internal_error, unauthenticated},
    platform::security::auth::{TotpConfig, TotpError},
};

const TOTP_SECRET_RAW_BYTES: usize = 20;
const ACCOUNT_RECOVERY_CODES_COUNT: usize = 8;
const ACCOUNT_RECOVERY_CODE_RAW_BYTES: usize = 10;
const ACCOUNT_RECOVERY_CODE_GROUP: usize = 4;
const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const DATA_RECOVERY_VERIFIER_HASH_DOMAIN: &[u8] = b"kamori.server.data-recovery-verifier.v1\0";

pub(crate) fn normalize_username(value: &str) -> Result<String, ApiError> {
    let username = value.trim().to_ascii_lowercase();
    let valid = (3..=64).contains(&username.len())
        && username
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && username.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        });
    if !valid {
        return Err(bad_request(
            "username must contain 3 to 64 lowercase letters, digits, '.', '_' or '-' and start with a letter or digit",
        ));
    }
    Ok(username)
}

pub(crate) fn hash_data_recovery_verifier(verifier: &[u8]) -> Result<Vec<u8>, ApiError> {
    if verifier.len() != 32 {
        return Err(bad_request("recovery_verifier must contain 32 bytes"));
    }
    let mut hasher = Sha256::new();
    hasher.update(DATA_RECOVERY_VERIFIER_HASH_DOMAIN);
    hasher.update(verifier);
    Ok(hasher.finalize().to_vec())
}

pub(crate) fn totp_issuer_from_config(config: &crate::platform::config::Config) -> String {
    let issuer = config.webauthn_rp_name.trim();
    if issuer.is_empty() {
        "Kamori".to_string()
    } else {
        issuer.to_string()
    }
}

pub(crate) fn map_totp_setup_error(error: TotpError) -> ApiError {
    match error {
        TotpError::InvalidBase32 => bad_request("invalid manual entry key"),
        TotpError::InvalidCode => bad_request("invalid totp code format"),
    }
}

pub(crate) fn map_totp_disable_error(error: TotpError) -> ApiError {
    match error {
        TotpError::InvalidBase32 => unauthenticated("invalid totp code"),
        TotpError::InvalidCode => bad_request("invalid totp code format"),
    }
}

pub(crate) fn build_totp_otpauth_uri(
    issuer: &str,
    username: &str,
    manual_entry_key: &str,
) -> Result<String, ApiError> {
    let label = format!("{}:{}", issuer.trim(), username.trim());
    let cfg = TotpConfig::default();
    let digits = cfg.digits.to_string();
    let period = cfg.step_seconds.to_string();
    let mut url = Url::parse("otpauth://totp").map_err(internal_error)?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| internal_error("failed to build otpauth uri"))?;
        segments.push(&label);
    }
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("secret", manual_entry_key)
            .append_pair("issuer", issuer.trim())
            .append_pair("algorithm", "SHA1")
            .append_pair("digits", &digits)
            .append_pair("period", &period);
    }
    Ok(url.to_string())
}

fn base32_encode_no_padding(raw: &[u8]) -> String {
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut bits_left: u8 = 0;

    for &byte in raw {
        buffer = (buffer << 8) | u32::from(byte);
        bits_left += 8;
        while bits_left >= 5 {
            bits_left -= 5;
            let index = ((buffer >> bits_left) & 0x1f) as usize;
            out.push(BASE32_ALPHABET[index] as char);
        }
    }

    if bits_left > 0 {
        let index = ((buffer << (5 - bits_left)) & 0x1f) as usize;
        out.push(BASE32_ALPHABET[index] as char);
    }

    out
}

pub(crate) fn generate_totp_manual_entry_key() -> String {
    let mut raw = [0u8; TOTP_SECRET_RAW_BYTES];
    let mut rng = rand::rng();
    rng.fill(&mut raw);
    base32_encode_no_padding(&raw)
}

pub(crate) fn normalize_totp_manual_entry_key(value: &str) -> Result<String, ApiError> {
    let normalized = value
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-')
        .map(|ch| ch.to_ascii_uppercase())
        .collect::<String>();
    if normalized.len() < 16 {
        return Err(bad_request("manual_entry_key is too short"));
    }
    if !normalized
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || matches!(ch, '2'..='7'))
    {
        return Err(bad_request("manual_entry_key must be base32"));
    }
    Ok(normalized)
}

fn generate_account_recovery_code_canonical() -> String {
    let mut raw = [0u8; ACCOUNT_RECOVERY_CODE_RAW_BYTES];
    let mut rng = rand::rng();
    rng.fill(&mut raw);
    base32_encode_no_padding(&raw)
}

pub(crate) fn format_account_recovery_code_display(canonical: &str) -> String {
    let mut output = String::new();
    for (index, chunk) in canonical
        .as_bytes()
        .chunks(ACCOUNT_RECOVERY_CODE_GROUP)
        .enumerate()
    {
        if index > 0 {
            output.push('-');
        }
        for &byte in chunk {
            output.push(byte as char);
        }
    }
    output
}

pub(crate) fn normalize_recovery_code(value: &str) -> Result<String, ApiError> {
    let normalized = value
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-')
        .map(|ch| ch.to_ascii_uppercase())
        .collect::<String>();
    if normalized.len() < 12 {
        return Err(bad_request("recovery_code is too short"));
    }
    if !normalized
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || matches!(ch, '2'..='7'))
    {
        return Err(bad_request("recovery_code must be base32"));
    }
    Ok(normalized)
}

pub(crate) fn hash_account_recovery_code(canonical: &str) -> Vec<u8> {
    Sha256::digest(canonical.as_bytes()).to_vec()
}

pub(crate) fn generate_account_recovery_code_batch() -> Vec<(String, Vec<u8>)> {
    let mut seen = HashSet::with_capacity(ACCOUNT_RECOVERY_CODES_COUNT);
    let mut out = Vec::with_capacity(ACCOUNT_RECOVERY_CODES_COUNT);
    while out.len() < ACCOUNT_RECOVERY_CODES_COUNT {
        let canonical = generate_account_recovery_code_canonical();
        if !seen.insert(canonical.clone()) {
            continue;
        }
        out.push((
            format_account_recovery_code_display(&canonical),
            hash_account_recovery_code(&canonical),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use sha2::Digest;

    use super::{
        build_totp_otpauth_uri, format_account_recovery_code_display,
        generate_account_recovery_code_batch, generate_totp_manual_entry_key,
        hash_data_recovery_verifier, normalize_recovery_code, normalize_totp_manual_entry_key,
        normalize_username,
    };

    const ACCOUNT_RECOVERY_CODES_COUNT: usize = 8;

    #[test]
    fn username_normalization_is_strict_and_canonical() {
        assert_eq!(normalize_username(" Alice-1 ").unwrap(), "alice-1");
        assert!(normalize_username("ab").is_err());
        assert!(normalize_username("bad name").is_err());
        assert!(normalize_username("_leading").is_err());
        assert!(normalize_username(&"a".repeat(65)).is_err());
    }

    #[test]
    fn recovery_verifier_hash_is_domain_separated_and_length_checked() {
        let hash = hash_data_recovery_verifier(&[7; 32]).unwrap();
        assert_eq!(hash.len(), 32);
        assert_ne!(hash, sha2::Sha256::digest([7; 32]).to_vec());
        assert!(hash_data_recovery_verifier(&[7; 31]).is_err());
    }

    #[test]
    fn totp_manual_key_normalization_accepts_common_formatting() {
        let normalized = normalize_totp_manual_entry_key("ab cd-ef23 gh45-ij67")
            .expect("manual key normalization should succeed");
        assert_eq!(normalized, "ABCDEF23GH45IJ67");
    }

    #[test]
    fn totp_manual_key_generation_is_base32() {
        let key = generate_totp_manual_entry_key();
        assert!(key.len() >= 16);
        assert!(
            key.chars()
                .all(|ch| ch.is_ascii_uppercase() || matches!(ch, '2'..='7'))
        );
    }

    #[test]
    fn totp_otpauth_uri_includes_required_fields() {
        let uri =
            build_totp_otpauth_uri("Kamori", "alice", "ABCDEFGHIJKLMNOP").expect("otpauth uri");
        assert!(uri.starts_with("otpauth://totp/"));
        assert!(uri.contains("secret=ABCDEFGHIJKLMNOP"));
        assert!(uri.contains("issuer=Kamori"));
        assert!(uri.contains("algorithm=SHA1"));
        assert!(uri.contains("digits=6"));
        assert!(uri.contains("period=30"));
    }

    #[test]
    fn recovery_code_normalization_accepts_grouped_form() {
        let normalized = normalize_recovery_code("abcd-efgh-ijkl-mnop")
            .expect("recovery code normalization should succeed");
        assert_eq!(normalized, "ABCDEFGHIJKLMNOP");
    }

    #[test]
    fn recovery_code_batch_has_expected_count_and_uniqueness() {
        let batch = generate_account_recovery_code_batch();
        assert_eq!(batch.len(), ACCOUNT_RECOVERY_CODES_COUNT);

        let unique_codes = batch
            .iter()
            .map(|(code, _hash)| code.clone())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique_codes.len(), ACCOUNT_RECOVERY_CODES_COUNT);
        assert!(batch.iter().all(|(_code, hash)| hash.len() == 32));
    }

    #[test]
    fn recovery_code_display_groups_in_chunks_of_four() {
        let display = format_account_recovery_code_display("ABCDEFGHIJKLMNOP");
        assert_eq!(display, "ABCD-EFGH-IJKL-MNOP");
    }

    #[test]
    fn recovery_code_normalization_rejects_non_base32_chars() {
        let err = normalize_recovery_code("abcd-efgh-ijkl-mno*").expect_err("must fail");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(err.1.0.message, "recovery_code must be base32");
    }
}
