use mazbri_core::num::SaturatingFrom;
mod cache;
mod config;
mod metrics;
mod permission;
mod store;
mod ttl;
pub use cache::{Cache, CacheEntry, MemoryCache};
pub use config::CacheConfig;
pub use mazbri_core::{CoreError, Result};
pub use metrics::{
    CACHE_ENTRIES, CACHE_MEMORY_BYTES, CacheOperationRecorder, EvictionRecorder,
    register_cache_metrics,
};
pub use permission::RedisPermissionCache;
use sqlx::{PgPool, Row};
use std::sync::Arc;
pub use store::{CacheStats, RedisCache};
pub use ttl::TtlManager;

#[derive(Clone)]
pub struct CacheService {
    db: Arc<PgPool>,
    config: Arc<CacheConfig>,
    cache: Arc<RedisCache>,
    ttl_manager: Arc<TtlManager>,
}

impl CacheService {
    #[must_use]
    pub fn new(
        db: Arc<PgPool>,
        config: CacheConfig,
        redis: mazbri_core::redis::RedisClient,
    ) -> Self {
        let cache = Arc::new(RedisCache::with_client(db.clone(), config.clone(), redis));
        let ttl_manager = Arc::new(TtlManager::new(cache.clone()));
        Self {
            db,
            config: Arc::new(config),
            cache,
            ttl_manager,
        }
    }
    #[must_use]
    pub fn cache(&self) -> &Arc<RedisCache> {
        &self.cache
    }
    #[must_use]
    pub fn config(&self) -> &Arc<CacheConfig> {
        &self.config
    }
    #[must_use]
    pub fn ttl_manager(&self) -> &Arc<TtlManager> {
        &self.ttl_manager
    }
    #[must_use]
    pub fn db(&self) -> &Arc<PgPool> {
        &self.db
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn load_config(
        &self,
        tenant_id: uuid::Uuid,
    ) -> mazbri_core::Result<Option<CacheConfig>> {
        let row = sqlx::query(
            r"
            SELECT
                enabled,
                default_ttl,
                max_cache_size,
                api_ttl,
                tenant_isolated
            FROM cache_configs
            WHERE tenant_id = $1
            ",
        )
        .bind(tenant_id)
        .fetch_optional(&*self.db)
        .await?;
        match row {
            Some(row) => {
                let enabled: bool = row.try_get("enabled")?;
                if !enabled {
                    return Ok(None);
                }
                let default_ttl: i32 = row.try_get("default_ttl")?;
                let max_cache_size: i32 = row.try_get("max_cache_size")?;
                let api_ttl: serde_json::Value = row.try_get("api_ttl")?;
                let tenant_isolated: bool = row.try_get("tenant_isolated")?;
                let api_ttl_map: std::collections::HashMap<String, u64> = if api_ttl.is_null() {
                    std::collections::HashMap::new()
                } else {
                    serde_json::from_value(api_ttl).unwrap_or_default()
                };
                Ok(Some(CacheConfig {
                    default_ttl: u64::saturating_from(default_ttl),
                    max_cache_size: u64::saturating_from(max_cache_size),
                    api_ttl: api_ttl_map,
                    tenant_isolated,
                    max_entry_size: 1_048_576,
                }))
            }
            None => Ok(None),
        }
    }
}
