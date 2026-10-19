use async_trait::async_trait;
use std::time::Duration;
use uuid::Uuid;
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionEntry {
    pub enabled: bool,
}
#[async_trait]
pub trait PermissionCache: Send + Sync + 'static {
    async fn get(
        &self,
        tenant_id: &Uuid,
        product_id: &str,
    ) -> anyhow::Result<Option<PermissionEntry>>;
    async fn set(
        &self,
        tenant_id: &Uuid,
        product_id: &str,
        entry: PermissionEntry,
        ttl: Duration,
    ) -> anyhow::Result<()>;
    async fn invalidate(&self, tenant_id: &Uuid, product_id: &str) -> anyhow::Result<()>;
}
