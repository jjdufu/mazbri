use super::cache::{Cache, CacheEntry, MemoryCache};
use super::config::CacheConfig;
use super::metrics::{
    CACHE_ENTRIES, CACHE_MEMORY_BYTES, CacheOperationRecorder, add_tenant_size_estimate,
    reset_tenant_size_estimate,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use mazbri_core::CoreError;
use mazbri_core::redis::{RedisClient, sha256_hex};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

const SHARED_MEMORY_NAMESPACE: &str = "__shared__";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RedisCacheEnvelope {
    value: String,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
    ttl_secs: u64,
}

impl RedisCacheEnvelope {
    fn new(value: Vec<u8>, ttl_secs: u64, metadata: serde_json::Value) -> Self {
        Self {
            value: STANDARD.encode(value),
            metadata,
            created_at: Utc::now(),
            ttl_secs,
        }
    }

    fn into_entry(self, ttl_secs: u64) -> Result<CacheEntry, CoreError> {
        let value = STANDARD.decode(&self.value).map_err(|e| {
            CoreError::Validation(format!(
                "Cache base64 decode failed: {e}. This indicates corrupted cache data."
            ))
        })?;
        Ok(CacheEntry {
            value,
            ttl: ttl_secs,
            expires_at: Utc::now() + chrono::Duration::seconds(ttl_secs as i64),
            metadata: self.metadata,
        })
    }
}

#[derive(Clone)]
pub struct RedisCache {
    db: Arc<PgPool>,
    config: Arc<CacheConfig>,
    memory_cache: Arc<MemoryCache>,
    redis: RedisClient,
}

impl RedisCache {
    fn redis_unavailable(error: impl std::fmt::Display) -> CoreError {
        CoreError::service_unavailable(format!("redis: {error}"))
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn new(
        db: Arc<PgPool>,
        config: CacheConfig,
        redis_url: &str,
    ) -> anyhow::Result<Self> {
        let redis = RedisClient::connect(redis_url).await?;
        Ok(Self::with_client(db, config, redis))
    }

    #[must_use]
    pub fn with_client(db: Arc<PgPool>, config: CacheConfig, redis: RedisClient) -> Self {
        let memory_cache = Arc::new(MemoryCache::new(10_000, config.max_entry_size));
        Self {
            db,
            config: Arc::new(config),
            memory_cache,
            redis,
        }
    }

    fn memory_namespace<'a>(config: &CacheConfig, tenant_id: &'a str) -> &'a str {
        if config.tenant_isolated {
            tenant_id
        } else {
            SHARED_MEMORY_NAMESPACE
        }
    }

    async fn resolve_config(
        &self,
        tenant_id: &str,
    ) -> Result<(CacheConfig, bool), mazbri_core::CoreError> {
        let tenant_uuid = match Uuid::parse_str(tenant_id) {
            Ok(uuid) => uuid,
            Err(_) => return Ok(((*self.config).clone(), true)),
        };
        let row = sqlx::query(
            r"
            SELECT enabled, default_ttl, max_cache_size, api_ttl, tenant_isolated
            FROM cache_configs
            WHERE tenant_id = $1
            ",
        )
        .bind(tenant_uuid)
        .fetch_optional(&*self.db)
        .await?;
        // No tenant cache_configs row → caching disabled (config is the control plane switch).
        let Some(row) = row else {
            return Ok(((*self.config).clone(), false));
        };
        let enabled: bool = row.try_get("enabled")?;
        let default_ttl: i32 = row.try_get("default_ttl")?;
        let max_cache_size: i32 = row.try_get("max_cache_size")?;
        let api_ttl: serde_json::Value = row.try_get("api_ttl")?;
        let tenant_isolated: bool = row.try_get("tenant_isolated")?;
        let api_ttl_map = if api_ttl.is_null() {
            std::collections::HashMap::new()
        } else {
            serde_json::from_value(api_ttl).unwrap_or_default()
        };
        Ok((
            CacheConfig {
                default_ttl: default_ttl as u64,
                max_cache_size: max_cache_size as u64,
                max_entry_size: self.config.max_entry_size,
                api_ttl: api_ttl_map,
                tenant_isolated,
            },
            enabled,
        ))
    }

    fn redis_key(&self, tenant_id: &str, key: &str) -> String {
        let cache_key = if self.config.tenant_isolated {
            key.to_string()
        } else {
            format!("shared:{key}")
        };
        format!(
            "{}:cache:{}:{}",
            self.redis.key_prefix(),
            tenant_id,
            sha256_hex(&cache_key)
        )
    }

    async fn encode_and_store(
        &self,
        tenant_id: &str,
        key: &str,
        value: Vec<u8>,
        ttl: std::time::Duration,
        metadata: serde_json::Value,
    ) -> Result<(), mazbri_core::CoreError> {
        let ttl_secs = ttl.as_secs().max(1);
        let redis_key = self.redis_key(tenant_id, key);
        let envelope = RedisCacheEnvelope::new(value, ttl_secs, metadata);
        let payload = serde_json::to_vec(&envelope)
            .map_err(|error| mazbri_core::CoreError::Validation(error.to_string()))?;
        self.redis
            .set_ex_bytes(&redis_key, &payload, ttl_secs as usize)
            .await
            .map_err(Self::redis_unavailable)?;
        Ok(())
    }

    async fn load_entry(
        &self,
        redis_key: &str,
        cached: Vec<u8>,
    ) -> Result<Option<CacheEntry>, mazbri_core::CoreError> {
        let envelope: RedisCacheEnvelope = serde_json::from_slice(&cached)
            .map_err(|error| mazbri_core::CoreError::Validation(error.to_string()))?;
        let ttl_secs = self.redis.ttl(redis_key).await.unwrap_or(-1);
        if ttl_secs <= 0 {
            return Ok(None);
        }
        let entry = envelope.into_entry(ttl_secs as u64)?;
        Ok(Some(entry))
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn clear_tenant_with_count(
        &self,
        tenant_id: &str,
    ) -> Result<u64, mazbri_core::CoreError> {
        let pattern = format!("{}:cache:{}:*", self.redis.key_prefix(), tenant_id);
        let keys = self
            .redis
            .scan_match(&pattern)
            .await
            .map_err(Self::redis_unavailable)?;
        let mut deleted = 0_u64;
        for chunk in keys.chunks(500) {
            let count = self
                .redis
                .del_multiple(chunk)
                .await
                .map_err(Self::redis_unavailable)?;
            deleted = deleted.saturating_add(count);
        }
        Ok(deleted)
    }

    /// Set a value, optionally resolving TTL from tenant `api_ttl` via `endpoint`.
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn set_with_endpoint(
        &self,
        tenant_id: &str,
        key: &str,
        value: Vec<u8>,
        ttl: std::time::Duration,
        endpoint: Option<&str>,
    ) -> Result<(), mazbri_core::CoreError> {
        let recorder = CacheOperationRecorder::new("set");
        let _timer = recorder.start_timer();
        let (config, enabled) = self.resolve_config(tenant_id).await?;
        if !enabled {
            recorder.record_success();
            return Ok(());
        }
        if value.len() as u64 > config.max_entry_size {
            recorder.record_error();
            return Err(mazbri_core::CoreError::Validation(format!(
                "缓存值大小超过限制 ({} bytes, max: {} bytes)",
                value.len(),
                config.max_entry_size
            )));
        }
        let effective_ttl = if ttl.is_zero() {
            if let Some(path) = endpoint.filter(|p| !p.is_empty()) {
                config.get_ttl(path)
            } else {
                std::time::Duration::from_secs(config.default_ttl)
            }
        } else {
            ttl
        };
        self.encode_and_store(
            tenant_id,
            key,
            value.clone(),
            effective_ttl,
            serde_json::json!({}),
        )
        .await?;
        let memory_tenant_id = Self::memory_namespace(&config, tenant_id);
        let entry = CacheEntry::new(
            value.clone(),
            effective_ttl.as_secs(),
            serde_json::json!({}),
        );
        let _ = self.memory_cache.insert(memory_tenant_id, key, entry).await;
        CACHE_ENTRIES.inc();
        CACHE_MEMORY_BYTES.add(value.len() as i64);
        add_tenant_size_estimate(tenant_id, value.len() as i64);
        recorder.record_success();
        Ok(())
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn stats(
        &self,
        tenant_id: Option<&str>,
    ) -> Result<CacheStats, mazbri_core::CoreError> {
        let pattern = match tenant_id {
            Some(tenant_id) => format!("{}:cache:{}:*", self.redis.key_prefix(), tenant_id),
            None => format!("{}:cache:*", self.redis.key_prefix()),
        };
        let keys = self
            .redis
            .scan_match(&pattern)
            .await
            .map_err(Self::redis_unavailable)?;
        let mut total_size = 0_u64;
        for chunk in keys.chunks(500) {
            if let Ok(usages) = self.redis.memory_usages(chunk).await {
                for size in usages.into_iter().flatten() {
                    total_size = total_size.saturating_add(size);
                }
            }
        }
        Ok(CacheStats {
            total_entries: keys.len() as u64,
            total_size,
            expired_entries: 0,
        })
    }
}

#[async_trait::async_trait]
impl Cache for RedisCache {
    async fn get(
        &self,
        tenant_id: &str,
        key: &str,
    ) -> Result<Option<CacheEntry>, mazbri_core::CoreError> {
        let recorder = CacheOperationRecorder::new("get");
        let _timer = recorder.start_timer();
        let (config, enabled) = self.resolve_config(tenant_id).await?;
        if !enabled {
            recorder.record_miss();
            return Ok(None);
        }
        let memory_tenant_id = Self::memory_namespace(&config, tenant_id);
        if let Some(entry) = self.memory_cache.get(memory_tenant_id, key).await? {
            if !entry.is_expired() {
                recorder.record_hit();
                return Ok(Some(entry));
            }
            let _ = self.memory_cache.remove(memory_tenant_id, key).await;
        }
        let redis_key = self.redis_key(tenant_id, key);
        let cached = self
            .redis
            .get_bytes(&redis_key)
            .await
            .map_err(Self::redis_unavailable)?;
        if let Some(bytes) = cached {
            if let Some(entry) = self.load_entry(&redis_key, bytes).await? {
                let _ = self
                    .memory_cache
                    .insert(memory_tenant_id, key, entry.clone())
                    .await;
                recorder.record_hit();
                Ok(Some(entry))
            } else {
                recorder.record_miss();
                Ok(None)
            }
        } else {
            recorder.record_miss();
            Ok(None)
        }
    }

    async fn set(
        &self,
        tenant_id: &str,
        key: &str,
        value: Vec<u8>,
        ttl: std::time::Duration,
    ) -> Result<(), mazbri_core::CoreError> {
        self.set_with_endpoint(tenant_id, key, value, ttl, None)
            .await
    }

    async fn delete(&self, tenant_id: &str, key: &str) -> Result<(), mazbri_core::CoreError> {
        let recorder = CacheOperationRecorder::new("delete");
        let _timer = recorder.start_timer();
        let (config, _) = self.resolve_config(tenant_id).await?;
        let redis_key = self.redis_key(tenant_id, key);
        let deleted = self
            .redis
            .del(&redis_key)
            .await
            .map_err(Self::redis_unavailable)?;
        let memory_tenant_id = Self::memory_namespace(&config, tenant_id);
        let removed = self.memory_cache.get(memory_tenant_id, key).await?;
        let _ = self.memory_cache.remove(memory_tenant_id, key).await;
        if deleted > 0 {
            CACHE_ENTRIES.dec();
            if let Some(entry) = removed {
                CACHE_MEMORY_BYTES.add(-(entry.value.len() as i64));
                reset_tenant_size_estimate(tenant_id);
            }
        }
        recorder.record_success();
        Ok(())
    }

    async fn clear_tenant(&self, tenant_id: &str) -> Result<(), mazbri_core::CoreError> {
        let recorder = CacheOperationRecorder::new("clear_tenant");
        let deleted = self.clear_tenant_with_count(tenant_id).await?;
        let _ = self.memory_cache.clear_tenant(tenant_id).await;
        reset_tenant_size_estimate(tenant_id);
        if deleted > 0 {
            recorder.record_success();
        } else {
            recorder.record_not_found();
        }
        Ok(())
    }

    async fn exists(&self, tenant_id: &str, key: &str) -> Result<bool, mazbri_core::CoreError> {
        let recorder = CacheOperationRecorder::new("exists");
        let _timer = recorder.start_timer();
        let (config, enabled) = self.resolve_config(tenant_id).await?;
        if !enabled {
            recorder.record_success();
            return Ok(false);
        }
        let redis_key = self.redis_key(tenant_id, key);
        let exists = self
            .redis
            .exists(&redis_key)
            .await
            .map_err(Self::redis_unavailable)?;
        let _ = Self::memory_namespace(&config, tenant_id);
        recorder.record_success();
        Ok(exists)
    }

    async fn get_multi(
        &self,
        tenant_id: &str,
        keys: &[String],
    ) -> Result<Vec<Option<CacheEntry>>, mazbri_core::CoreError> {
        let recorder = CacheOperationRecorder::new("get_multi");
        let _timer = recorder.start_timer();
        let (config, enabled) = self.resolve_config(tenant_id).await?;
        if !enabled {
            recorder.record_success();
            return Ok(vec![None; keys.len()]);
        }
        let memory_tenant_id = Self::memory_namespace(&config, tenant_id);
        let redis_keys: Vec<String> = keys
            .iter()
            .map(|key| self.redis_key(tenant_id, key))
            .collect();
        let values = self
            .redis
            .mget_bytes(&redis_keys)
            .await
            .map_err(Self::redis_unavailable)?;
        let mut results = Vec::with_capacity(keys.len());
        for (index, value) in values.into_iter().enumerate() {
            match value {
                Some(bytes) => match self.load_entry(&redis_keys[index], bytes).await? {
                    Some(entry) => {
                        let _ = self
                            .memory_cache
                            .insert(memory_tenant_id, &keys[index], entry.clone())
                            .await;
                        results.push(Some(entry));
                    }
                    None => results.push(None),
                },
                None => results.push(None),
            }
        }
        recorder.record_success();
        Ok(results)
    }

    async fn set_multi(
        &self,
        tenant_id: &str,
        entries: Vec<(String, Vec<u8>, std::time::Duration)>,
    ) -> Result<(), mazbri_core::CoreError> {
        let recorder = CacheOperationRecorder::new("set_multi");
        let _timer = recorder.start_timer();
        let (config, enabled) = self.resolve_config(tenant_id).await?;
        if !enabled || entries.is_empty() {
            recorder.record_success();
            return Ok(());
        }
        let memory_tenant_id = Self::memory_namespace(&config, tenant_id);
        for (key, value, ttl) in entries {
            if value.len() as u64 > config.max_entry_size {
                recorder.record_error();
                return Err(mazbri_core::CoreError::Validation(format!(
                    "缓存值大小超过限制 ({} bytes, max: {} bytes)",
                    value.len(),
                    config.max_entry_size
                )));
            }
            let effective_ttl = if ttl.is_zero() {
                std::time::Duration::from_secs(config.default_ttl)
            } else {
                ttl
            };
            self.encode_and_store(
                tenant_id,
                &key,
                value.clone(),
                effective_ttl,
                serde_json::json!({}),
            )
            .await?;
            let entry = CacheEntry::new(
                value.clone(),
                effective_ttl.as_secs(),
                serde_json::json!({}),
            );
            let _ = self
                .memory_cache
                .insert(memory_tenant_id, &key, entry)
                .await;
            CACHE_ENTRIES.inc();
            CACHE_MEMORY_BYTES.add(value.len() as i64);
            add_tenant_size_estimate(tenant_id, value.len() as i64);
        }
        recorder.record_success();
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub total_entries: u64,
    pub total_size: u64,
    pub expired_entries: u64,
}
