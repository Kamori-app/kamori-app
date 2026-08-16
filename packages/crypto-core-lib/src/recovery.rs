//! Offline account-data recovery kit encoding.

use bip39::{Language, Mnemonic};

/// Encodes the account master key as a checksummed 24-word English BIP-39 phrase.
pub fn encode_master_key(master_key: &[u8; 32]) -> anyhow::Result<String> {
    Ok(Mnemonic::from_entropy(master_key)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
        .to_string())
}

/// Restores the exact 256-bit account master key from a normalized 24-word phrase.
pub fn decode_master_key(phrase: &str) -> anyhow::Result<[u8; 32]> {
    let normalized = phrase.split_whitespace().collect::<Vec<_>>().join(" ");
    let mnemonic = Mnemonic::parse_in(Language::English, normalized)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    if mnemonic.word_count() != 24 {
        anyhow::bail!("recovery kit must contain exactly 24 words");
    }
    mnemonic
        .to_entropy()
        .try_into()
        .map_err(|_| anyhow::anyhow!("recovery kit entropy must be 32 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_kit_roundtrips_master_key() {
        let key = [0x5a; 32];
        let phrase = encode_master_key(&key).expect("encode recovery kit");
        assert_eq!(phrase.split_whitespace().count(), 24);
        assert_eq!(
            decode_master_key(&phrase).expect("decode recovery kit"),
            key
        );
    }

    #[test]
    fn recovery_kit_rejects_short_or_invalid_phrases() {
        assert!(decode_master_key("abandon abandon abandon").is_err());
        let mut words = encode_master_key(&[7; 32])
            .expect("encode recovery kit")
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        words[23] = "zoo".to_owned();
        assert!(decode_master_key(&words.join(" ")).is_err());
    }
}
