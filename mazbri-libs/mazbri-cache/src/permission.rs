use async_trait::async_trait;
use mazbri_auth::{PermissionCache, PermissionEntry};
use mazbri_core::redis::RedisClient;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone)]
pub struct RedisPermissionCache {
    redis: RedisClient,
    default_ttl: Duration,
}

impl RedisPermissionCache {
    #[must_use]
    pub fn new(redis: RedisClient, default_ttl: Duration) -> Self {
        Self { redis, default_ttl }
    }

    fn cache_key(&self, tenant_id: &Uuid, product_id: &str) -> String {
        self.redis.permission_key(*tenant_id, product_id)
    }

    #[must_use]
    pub fn default_ttl(&self) -> Duration {
        self.default_ttl
    }
}

#[async_trait]
impl PermissionCache for RedisPermissionCache {
    async fn get(
        &self,
        tenant_id: &Uuid,
        product_id: &str,
    ) -> anyhow::Result<Option<PermissionEntry>> {
        let cache_key = self.cache_key(tenant_id, product_id);
        let value = self.redis.get_bytes(&cache_key).await?;
        match value {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    async fn set(
        &self,
        tenant_id: &Uuid,
        product_id: &str,
        entry: PermissionEntry,
        ttl: Duration,
    ) -> anyhow::Result<()> {
        let cache_key = self.cache_key(tenant_id, product_id);
        let ttl_secs = ttl.as_secs().max(self.default_ttl.as_secs()).max(1);
        let payload = serde_json::to_vec(&entry)?;
        self.redis
            .set_ex_bytes(&cache_key, &payload, ttl_secs as usize)
            .await?;
        Ok(())
    }

    async fn invalidate(&self, tenant_id: &Uuid, product_id: &str) -> anyhow::Result<()> {
        let cache_key = self.cache_key(tenant_id, product_id);
        let _ = self.redis.del(&cache_key).await?;
        Ok(())
    }
}
