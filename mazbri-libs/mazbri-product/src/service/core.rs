use super::{Arc, Cache, Duration, PermissionCache, PgPool, ProductConfig, ProductService};

impl ProductService {
    pub fn new(
        db: PgPool,
        config: ProductConfig,
        permission_cache: Arc<dyn PermissionCache>,
    ) -> Self {
        Self {
            db,
            tenant_product_detail_cache: Cache::builder()
                .max_capacity(config.cache_size as u64)
                .time_to_live(Duration::from_secs(config.cache_ttl_secs))
                .build(),
            config,
            permission_cache,
        }
    }
}
