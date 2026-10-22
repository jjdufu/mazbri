use super::store::{CacheStats, RedisCache};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct TtlManager {
    cache: Arc<RedisCache>,
    shutdown_tx: broadcast::Sender<()>,
}

impl TtlManager {
    #[must_use]
    pub fn new(cache: Arc<RedisCache>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self { cache, shutdown_tx }
    }

    pub async fn start(self) -> (tokio::task::JoinHandle<()>, broadcast::Sender<()>) {
        let shutdown_tx = self.shutdown_tx.clone();
        let mut shutdown_rx = shutdown_tx.subscribe();
        let handle = tokio::spawn(async move {
            let _ = shutdown_rx.recv().await;
            tracing::info!("Redis cache TTL manager stopped");
        });
        (handle, shutdown_tx)
    }

    pub fn stop(&self) {
        let _ = self.shutdown_tx.send(());
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn cleanup_expired(&self) -> Result<u64, mazbri_core::CoreError> {
        Ok(0)
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
