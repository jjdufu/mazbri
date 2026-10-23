use super::store::{CacheStats, RedisCache};
use std::sync::Arc;

#[derive(Clone)]
pub struct TtlManager {
    cache: Arc<RedisCache>,
}

impl TtlManager {
    #[must_use]
    pub fn new(cache: Arc<RedisCache>) -> Self {
        Self { cache }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_stats(&self, tenant_id: &str) -> Result<CacheStats, mazbri_core::CoreError> {
        self.cache.stats(Some(tenant_id)).await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_global_stats(&self) -> Result<CacheStats, mazbri_core::CoreError> {
        self.cache.stats(None).await
    }
}
