use mazbri_product::model::ProductRoute;
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};
#[derive(Clone)]
pub struct ProductCacheConfig {
    pub max_capacity: u64,
    pub ttl_secs: u64,
    pub tti_secs: u64,
}
impl Default for ProductCacheConfig {
    fn default() -> Self {
        Self {
            max_capacity: 1000,
            ttl_secs: 5,
            tti_secs: 10,
        }
    }
}
pub struct ProductCache {
    cache: Cache<String, Arc<ProductRoute>>,
}
impl ProductCache {
    pub fn new(config: ProductCacheConfig) -> Self {
        info!(
            "Creating L1 product cache: capacity={}, TTL={}s, TTI={}s", config
            .max_capacity, config.ttl_secs, config.tti_secs
        );
        let cache = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(Duration::from_secs(config.ttl_secs))
            .time_to_idle(Duration::from_secs(config.tti_secs))
            .build();
        Self { cache }
    }
    pub async fn get(&self, product_id: &str) -> Option<Arc<ProductRoute>> {
        self.cache.get(product_id).await
    }
    pub async fn insert(&self, product_id: String, route: Arc<ProductRoute>) {
        debug!("Cache insert: product_id={}", product_id);
        self.cache.insert(product_id, route).await;
    }
    pub async fn invalidate(&self, product_id: &str) {
        debug!("Cache invalidate: product_id={}", product_id);
        self.cache.invalidate(product_id).await;
    }
    pub fn invalidate_all(&self) {
        debug!("Cache invalidate: ALL");
        self.cache.invalidate_all();
    }
    pub fn stats(&self) -> CacheStats {
        let sized = self.cache.entry_count();
        let max_capacity = self.cache.weighted_size();
        CacheStats {
            current_size: sized,
            max_capacity,
        }
    }
}
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub current_size: u64,
    pub max_capacity: u64,
}
