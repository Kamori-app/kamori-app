//! Space-scoped encrypted blob transport DTOs.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CasUploadRequest {
    pub blob_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub ciphertext_sha256: Vec<u8>,
    pub size_padded: u64,
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CasUploadResponse {
    pub blob_id: Uuid,
    pub stored: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cas_upload_msgpack_roundtrip_preserves_bytes() {
        let request = CasUploadRequest {
            blob_id: Uuid::new_v4(),
            ciphertext_sha256: vec![7; 32],
            size_padded: 4,
            data: vec![0, 1, 2, 255],
        };
        let encoded = rmp_serde::to_vec_named(&request).expect("encode");
        let decoded: CasUploadRequest = rmp_serde::from_slice(&encoded).expect("decode");
        assert_eq!(decoded.blob_id, request.blob_id);
        assert_eq!(decoded.ciphertext_sha256, vec![7; 32]);
        assert_eq!(decoded.data, request.data);
    }
}
