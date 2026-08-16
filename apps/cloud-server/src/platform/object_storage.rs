//! S3-compatible ciphertext object storage.

use std::sync::Arc;

use anyhow::{Context, Result};
use object_store::{ObjectStore, aws::AmazonS3Builder, memory::InMemory};

use crate::platform::config::Config;

/// Builds the configured primary object store.
pub fn build_object_store(config: &Config) -> Result<Arc<dyn ObjectStore>> {
    if config.object_store_endpoint == "memory://" {
        return Ok(Arc::new(InMemory::new()));
    }

    let store = AmazonS3Builder::new()
        .with_endpoint(&config.object_store_endpoint)
        .with_region(&config.object_store_region)
        .with_bucket_name(&config.object_store_bucket)
        .with_access_key_id(&config.object_store_access_key_id)
        .with_secret_access_key(&config.object_store_secret_access_key)
        .with_allow_http(config.object_store_allow_http)
        .with_virtual_hosted_style_request(config.object_store_virtual_hosted_style)
        .build()
        .context("build S3-compatible object store")?;
    Ok(Arc::new(store))
}

#[cfg(test)]
mod tests {
    use object_store::{ObjectStoreExt, PutPayload, path::Path};

    use super::*;
    use crate::platform::test_support::test_config;

    #[tokio::test]
    async fn memory_store_roundtrip() {
        let store = build_object_store(&test_config()).expect("store");
        let path = Path::from("spaces/test/blob");
        store
            .put(&path, PutPayload::from(vec![1, 2, 3]))
            .await
            .expect("put");
        let bytes = store.get(&path).await.expect("get").bytes().await.unwrap();
        assert_eq!(bytes.as_ref(), &[1, 2, 3]);
    }
}
