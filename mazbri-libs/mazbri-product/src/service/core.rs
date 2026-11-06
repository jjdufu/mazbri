use super::{
    Arc, Cache, Duration, PermissionCache, PgPool, ProductConfig, ProductService,
};
use super::tenant_cache_enabled::TenantCacheEnabledLookup;

impl ProductService {
    /// Build a product service.
    ///
    /// `cache_db` is the cache-service Postgres pool (`cache_configs`). When
    /// `None`, snapshot `response_cache.enabled` stays false (skip cache RPC).
    #[must_use]
    pub fn new(
        db: PgPool,
        config: ProductConfig,
        permission_cache: Arc<dyn PermissionCache>,
        cache_db: Option<PgPool>,
    ) -> Self {
        Self {
            db,
            tenant_product_detail_cache: Cache::builder()
                .max_capacity(config.cache_size as u64)
                .time_to_live(Duration::from_secs(config.cache_ttl_secs))
                .build(),
            config,
            permission_cache,
            cache_enabled_lookup: TenantCacheEnabledLookup::new(cache_db),
        }
    }

    /// Drop a tenant's remembered `cache_configs.enabled` so the next snapshot
    /// reload hits Postgres (mgmt write path).
    pub fn invalidate_cache_enabled(&self, tenant_id: uuid::Uuid) {
        self.cache_enabled_lookup.invalidate(tenant_id);
    }
}
