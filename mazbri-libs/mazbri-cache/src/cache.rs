use super::metrics::EvictionRecorder;
use chrono::{DateTime, Utc};
use lru::LruCache;
use mazbri_core::num::SaturatingFrom;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::RwLock;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub value: Vec<u8>,
    pub ttl: u64,
    pub expires_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}
impl CacheEntry {
    #[must_use]
    pub fn new(value: Vec<u8>, ttl: u64, metadata: serde_json::Value) -> Self {
        let expires_at = Utc::now() + chrono::Duration::seconds(i64::saturating_from(ttl));
        Self {
            value,
            ttl,
            expires_at,
            metadata,
        }
    }
    #[must_use]
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}
#[async_trait::async_trait]
pub trait Cache: Send + Sync {
    async fn get(
        &self,
        tenant_id: &str,
        key: &str,
    ) -> Result<Option<CacheEntry>, mazbri_core::CoreError>;
    async fn set(
        &self,
        tenant_id: &str,
        key: &str,
        value: Vec<u8>,
        ttl: std::time::Duration,
    ) -> Result<(), mazbri_core::CoreError>;
    async fn delete(&self, tenant_id: &str, key: &str) -> Result<(), mazbri_core::CoreError>;
    async fn clear_tenant(&self, tenant_id: &str) -> Result<(), mazbri_core::CoreError>;
    async fn exists(&self, tenant_id: &str, key: &str) -> Result<bool, mazbri_core::CoreError>;
    async fn get_multi(
        &self,
        tenant_id: &str,
        keys: &[String],
    ) -> Result<Vec<Option<CacheEntry>>, mazbri_core::CoreError>;
    async fn set_multi(
        &self,
        tenant_id: &str,
        entries: Vec<(String, Vec<u8>, std::time::Duration)>,
    ) -> Result<(), mazbri_core::CoreError>;
}
#[derive(Clone)]
pub struct MemoryCache {
    data: Arc<RwLock<LruCache<String, CacheEntry>>>,
    max_entry_size: u64,
}
impl MemoryCache {
    /// # Panics
    ///
    /// 内部不变量被破坏时 panic（正常输入不会触发）。
    #[must_use]
    pub fn new(max_entries: usize, max_entry_size: u64) -> Self {
        let capacity = NonZeroUsize::new(max_entries).unwrap_or_else(|| {
            tracing::warn!(
                max_entries,
                "max_entries must be > 0, falling back to default capacity of 100"
            );
            NonZeroUsize::new(100).expect("100 is non-zero")
        });
        Self {
            data: Arc::new(RwLock::new(LruCache::new(capacity))),
            max_entry_size,
        }
    }
    fn make_key(tenant_id: &str, key: &str) -> String {
        format!("{tenant_id}:{key}")
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn insert(
        &self,
        tenant_id: &str,
        key: &str,
        entry: CacheEntry,
    ) -> Result<(), mazbri_core::CoreError> {
        if entry.value.len() as u64 > self.max_entry_size {
            return Err(mazbri_core::CoreError::Validation(format!(
                "缓存值大小超过限制 ({} bytes, max: {} bytes)",
                entry.value.len(),
                self.max_entry_size
            )));
        }
        let cache_key = Self::make_key(tenant_id, key);
        let mut cache = self.data.write().await;
        let evicted = cache.put(cache_key, entry);
        if evicted.is_some() {
            EvictionRecorder::new("lru").record();
        }
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get(
        &self,
        tenant_id: &str,
        key: &str,
    ) -> Result<Option<CacheEntry>, mazbri_core::CoreError> {
        let cache_key = Self::make_key(tenant_id, key);
        let mut cache = self.data.write().await;
        Ok(cache.get(&cache_key).cloned())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn remove(&self, tenant_id: &str, key: &str) -> Result<(), mazbri_core::CoreError> {
        let cache_key = Self::make_key(tenant_id, key);
        let mut cache = self.data.write().await;
        cache.pop(&cache_key);
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn clear_tenant(&self, tenant_id: &str) -> Result<(), mazbri_core::CoreError> {
        let prefix = format!("{tenant_id}:");
        let mut cache = self.data.write().await;
        let keys_to_remove: Vec<String> = cache
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys_to_remove {
            cache.pop(&key);
        }
        Ok(())
    }
    pub async fn len(&self) -> usize {
        self.data.read().await.len()
    }
    pub async fn is_empty(&self) -> bool {
        self.data.read().await.is_empty()
    }
}
