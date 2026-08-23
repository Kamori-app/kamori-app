//! Valkey-backed state store for ephemeral auth states.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Errors produced by the state store.
#[derive(Debug, thiserror::Error)]
pub enum StateStoreError {
    /// Backend error wrapper.
    #[error("backend error: {0}")]
    Backend(String),
    /// Serialization error wrapper.
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Configuration for Valkey state storage.
#[derive(Clone, Debug)]
pub struct ValkeyConfig {
    /// Connection URL (e.g. valkey://host:6379/0).
    pub url: String,
    /// Key prefix for namespacing.
    pub key_prefix: String,
    /// Default TTL for stored states.
    pub default_ttl: Duration,
}

impl ValkeyConfig {
    /// Builds a new config with a prefix and ttl.
    pub fn new(
        url: impl Into<String>,
        key_prefix: impl Into<String>,
        default_ttl: Duration,
    ) -> Self {
        Self {
            url: url.into(),
            key_prefix: key_prefix.into(),
            default_ttl,
        }
    }
}

/// A simple interface for storing ephemeral auth state.
#[async_trait::async_trait]
pub trait StateStore: Send + Sync {
    /// Stores a value by key with a TTL.
    async fn put(&self, key: &str, value: &[u8], ttl: Duration) -> Result<(), StateStoreError>;

    /// Stores a value only when the key does not already exist.
    async fn put_if_absent(
        &self,
        key: &str,
        value: &[u8],
        ttl: Duration,
    ) -> Result<bool, StateStoreError>;

    /// Loads a value by key.
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StateStoreError>;

    /// Deletes a value by key.
    async fn delete(&self, key: &str) -> Result<(), StateStoreError>;

    /// Atomically removes and returns a value when it exists and is unexpired.
    async fn take(&self, key: &str) -> Result<Option<Vec<u8>>, StateStoreError>;

    /// Replaces an unexpired value only when its bytes still match `expected`.
    async fn compare_and_set(
        &self,
        key: &str,
        expected: &[u8],
        value: &[u8],
        ttl: Duration,
    ) -> Result<bool, StateStoreError>;

    /// Atomically increments a counter and applies TTL when it is first created.
    async fn increment(&self, key: &str, ttl: Duration) -> Result<u64, StateStoreError>;

    /// Atomically adds weighted request units and sets TTL on first creation.
    async fn increment_by(
        &self,
        key: &str,
        amount: u64,
        ttl: Duration,
    ) -> Result<u64, StateStoreError>;
}

/// Valkey-backed state store implementation.
pub struct ValkeyStore {
    client: redis::Client,
    prefix: String,
    default_ttl: Duration,
}

impl ValkeyStore {
    /// Creates a new Valkey store from config.
    pub fn new(config: ValkeyConfig) -> Result<Self, StateStoreError> {
        let client = redis::Client::open(config.url.clone())
            .map_err(|e| StateStoreError::Backend(e.to_string()))?;
        Ok(Self {
            client,
            prefix: config.key_prefix,
            default_ttl: config.default_ttl,
        })
    }

    /// Builds a namespaced key.
    fn key(&self, raw: &str) -> String {
        format!("{}{}", self.prefix, raw)
    }
}

#[async_trait::async_trait]
impl StateStore for ValkeyStore {
    async fn put(&self, key: &str, value: &[u8], ttl: Duration) -> Result<(), StateStoreError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))?;

        let ttl = if ttl.is_zero() { self.default_ttl } else { ttl };
        let ttl_seconds = ttl.as_secs().max(1);

        let _: () = redis::cmd("SET")
            .arg(self.key(key))
            .arg(value)
            .arg("EX")
            .arg(ttl_seconds)
            .query_async(&mut conn)
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))?;

        Ok(())
    }

    async fn put_if_absent(
        &self,
        key: &str,
        value: &[u8],
        ttl: Duration,
    ) -> Result<bool, StateStoreError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))?;
        let ttl = if ttl.is_zero() { self.default_ttl } else { ttl };
        let result: Option<String> = redis::cmd("SET")
            .arg(self.key(key))
            .arg(value)
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs().max(1))
            .query_async(&mut conn)
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))?;
        Ok(result.is_some())
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StateStoreError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))?;

        let value: Option<Vec<u8>> = redis::cmd("GET")
            .arg(self.key(key))
            .query_async(&mut conn)
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))?;

        Ok(value)
    }

    async fn delete(&self, key: &str) -> Result<(), StateStoreError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))?;

        let _: () = redis::cmd("DEL")
            .arg(self.key(key))
            .query_async(&mut conn)
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))?;

        Ok(())
    }

    async fn take(&self, key: &str) -> Result<Option<Vec<u8>>, StateStoreError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))?;
        redis::cmd("GETDEL")
            .arg(self.key(key))
            .query_async(&mut conn)
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))
    }

    async fn compare_and_set(
        &self,
        key: &str,
        expected: &[u8],
        value: &[u8],
        ttl: Duration,
    ) -> Result<bool, StateStoreError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))?;
        let ttl = if ttl.is_zero() { self.default_ttl } else { ttl };
        let script = redis::Script::new(
            "if redis.call('GET', KEYS[1]) == ARGV[1] then redis.call('SET', KEYS[1], ARGV[2], 'EX', ARGV[3]); return 1 else return 0 end",
        );
        let changed: i64 = script
            .key(self.key(key))
            .arg(expected)
            .arg(value)
            .arg(ttl.as_secs().max(1))
            .invoke_async(&mut conn)
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))?;
        Ok(changed == 1)
    }

    async fn increment(&self, key: &str, ttl: Duration) -> Result<u64, StateStoreError> {
        self.increment_by(key, 1, ttl).await
    }

    async fn increment_by(
        &self,
        key: &str,
        amount: u64,
        ttl: Duration,
    ) -> Result<u64, StateStoreError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))?;
        let ttl = if ttl.is_zero() { self.default_ttl } else { ttl };
        let ttl_seconds = ttl.as_secs().max(1);
        let script = redis::Script::new(
            "local existed = redis.call('EXISTS', KEYS[1]); local value = redis.call('INCRBY', KEYS[1], ARGV[2]); if existed == 0 then redis.call('EXPIRE', KEYS[1], ARGV[1]); end; return value",
        );
        script
            .key(self.key(key))
            .arg(ttl_seconds)
            .arg(amount)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| StateStoreError::Backend(e.to_string()))
    }
}

/// In-memory state store for unit tests.
#[derive(Clone, Default)]
pub struct InMemoryStore {
    inner: Arc<RwLock<HashMap<String, MemoryEntry>>>,
    default_ttl: Duration,
}

#[derive(Clone)]
struct MemoryEntry {
    value: Vec<u8>,
    expires_at: Instant,
}

impl InMemoryStore {
    /// Creates a new in-memory store with a default TTL.
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            default_ttl,
        }
    }
}

#[async_trait::async_trait]
impl StateStore for InMemoryStore {
    async fn put(&self, key: &str, value: &[u8], ttl: Duration) -> Result<(), StateStoreError> {
        let ttl = if ttl.is_zero() { self.default_ttl } else { ttl };
        let expires_at = Instant::now() + ttl;
        let mut guard = self.inner.write().await;
        guard.insert(
            key.to_string(),
            MemoryEntry {
                value: value.to_vec(),
                expires_at,
            },
        );
        Ok(())
    }

    async fn put_if_absent(
        &self,
        key: &str,
        value: &[u8],
        ttl: Duration,
    ) -> Result<bool, StateStoreError> {
        let ttl = if ttl.is_zero() { self.default_ttl } else { ttl };
        let mut guard = self.inner.write().await;
        if guard
            .get(key)
            .is_some_and(|entry| entry.expires_at > Instant::now())
        {
            return Ok(false);
        }
        guard.insert(
            key.to_string(),
            MemoryEntry {
                value: value.to_vec(),
                expires_at: Instant::now() + ttl,
            },
        );
        Ok(true)
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StateStoreError> {
        let mut guard = self.inner.write().await;
        if let Some(entry) = guard.get(key) {
            if entry.expires_at <= Instant::now() {
                guard.remove(key);
                return Ok(None);
            }
            return Ok(Some(entry.value.clone()));
        }
        Ok(None)
    }

    async fn delete(&self, key: &str) -> Result<(), StateStoreError> {
        let mut guard = self.inner.write().await;
        guard.remove(key);
        Ok(())
    }

    async fn take(&self, key: &str) -> Result<Option<Vec<u8>>, StateStoreError> {
        let mut guard = self.inner.write().await;
        let Some(entry) = guard.remove(key) else {
            return Ok(None);
        };
        if entry.expires_at <= Instant::now() {
            return Ok(None);
        }
        Ok(Some(entry.value))
    }

    async fn compare_and_set(
        &self,
        key: &str,
        expected: &[u8],
        value: &[u8],
        ttl: Duration,
    ) -> Result<bool, StateStoreError> {
        let ttl = if ttl.is_zero() { self.default_ttl } else { ttl };
        let mut guard = self.inner.write().await;
        let now = Instant::now();
        let matches = guard
            .get(key)
            .is_some_and(|entry| entry.expires_at > now && entry.value == expected);
        if !matches {
            if guard.get(key).is_some_and(|entry| entry.expires_at <= now) {
                guard.remove(key);
            }
            return Ok(false);
        }
        guard.insert(
            key.to_string(),
            MemoryEntry {
                value: value.to_vec(),
                expires_at: now + ttl,
            },
        );
        Ok(true)
    }

    async fn increment(&self, key: &str, ttl: Duration) -> Result<u64, StateStoreError> {
        self.increment_by(key, 1, ttl).await
    }

    async fn increment_by(
        &self,
        key: &str,
        amount: u64,
        ttl: Duration,
    ) -> Result<u64, StateStoreError> {
        let ttl = if ttl.is_zero() { self.default_ttl } else { ttl };
        let mut guard = self.inner.write().await;
        let now = Instant::now();
        let entry = guard.entry(key.to_string()).or_insert_with(|| MemoryEntry {
            value: b"0".to_vec(),
            expires_at: now + ttl,
        });
        if entry.expires_at <= now {
            entry.value = b"0".to_vec();
            entry.expires_at = now + ttl;
        }
        let current = std::str::from_utf8(&entry.value)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0)
            .saturating_add(amount);
        entry.value = current.to_string().into_bytes();
        Ok(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn in_memory_store_put_get() {
        let store = InMemoryStore::new(Duration::from_secs(60));
        store.put("k", b"v", Duration::from_secs(1)).await.unwrap();
        let got = store.get("k").await.unwrap();
        assert_eq!(got, Some(b"v".to_vec()));
    }

    #[tokio::test]
    async fn in_memory_store_expires() {
        let store = InMemoryStore::new(Duration::from_millis(5));
        store
            .put("k", b"v", Duration::from_millis(5))
            .await
            .unwrap();
        sleep(Duration::from_millis(10)).await;
        let got = store.get("k").await.unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn in_memory_store_delete() {
        let store = InMemoryStore::new(Duration::from_secs(60));
        store.put("k", b"v", Duration::from_secs(1)).await.unwrap();
        store.delete("k").await.unwrap();
        let got = store.get("k").await.unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn in_memory_store_take_is_atomic_and_one_time() {
        let store = InMemoryStore::new(Duration::from_secs(60));
        store.put("k", b"v", Duration::from_secs(1)).await.unwrap();
        assert_eq!(store.take("k").await.unwrap(), Some(b"v".to_vec()));
        assert_eq!(store.take("k").await.unwrap(), None);
    }

    #[tokio::test]
    async fn in_memory_store_conditional_mutations_are_atomic() {
        let store = InMemoryStore::new(Duration::from_secs(60));
        assert!(
            store
                .put_if_absent("k", b"one", Duration::from_secs(1))
                .await
                .unwrap()
        );
        assert!(
            !store
                .put_if_absent("k", b"two", Duration::from_secs(1))
                .await
                .unwrap()
        );
        assert!(
            !store
                .compare_and_set("k", b"wrong", b"two", Duration::from_secs(1))
                .await
                .unwrap()
        );
        assert!(
            store
                .compare_and_set("k", b"one", b"two", Duration::from_secs(1))
                .await
                .unwrap()
        );
        assert_eq!(store.get("k").await.unwrap(), Some(b"two".to_vec()));
    }

    #[tokio::test]
    async fn in_memory_counter_increments_and_expires() {
        let store = InMemoryStore::new(Duration::from_secs(60));
        assert_eq!(
            store
                .increment("counter", Duration::from_millis(5))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .increment("counter", Duration::from_millis(5))
                .await
                .unwrap(),
            2
        );
        sleep(Duration::from_millis(10)).await;
        assert_eq!(
            store
                .increment("counter", Duration::from_millis(5))
                .await
                .unwrap(),
            1
        );
    }
}
