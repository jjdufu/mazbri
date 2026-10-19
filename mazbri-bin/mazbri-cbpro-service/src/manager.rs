use crate::cache::{HybridFallbackCache, SetCacheEntryParams};
use crate::config::CBProServiceConfig;
use crate::db::RedisFallbackRepository;
use mazbri_core::metrics::CbProMetricsRecorder;
use mazbri_rpc::{
    FallbackRequest, FallbackResponse, FallbackSource, RpcError, StoreRequest, StoreResponse,
};
use std::sync::Arc;
use tracing::{debug, info, warn};
pub struct FallbackManager {
    cache: Arc<HybridFallbackCache>,
}
impl FallbackManager {
    pub fn new(config: CBProServiceConfig, redis: Option<Arc<RedisFallbackRepository>>) -> Self {
        let cache = Arc::new(HybridFallbackCache::new(&config.cache, redis));
        CbProMetricsRecorder::record_state_transition("init", "closed");
        CbProMetricsRecorder::set_current_state(0);
        Self { cache }
    }
    pub async fn get_fallback(&self, request: FallbackRequest) -> FallbackResponse {
        let cache_key =
            Self::build_cache_key(&request.product_id, &request.tenant_id, &request.cache_key);
        debug!(
            product_id = % request.product_id, tenant_id = % request.tenant_id, cache_key
            = % cache_key, "Looking up fallback data"
        );
        if let Some(entry) = self.cache.get(&cache_key).await {
            let age_secs = entry.age_secs();
            info!(
                product_id = % request.product_id, source = "hybrid_cache", age_secs =
                age_secs, "Found fallback data in cache"
            );
            CbProMetricsRecorder::record_protection_trigger("fallback_hit");
            return FallbackResponse {
                available: true,
                data: Some(entry.data),
                source: FallbackSource::MemoryCache,
                age_secs: Some(age_secs),
                error: None,
            };
        }
        warn!(
            product_id = % request.product_id, tenant_id = % request.tenant_id,
            "No fallback data available"
        );
        CbProMetricsRecorder::record_protection_trigger("fallback_miss");
        FallbackResponse {
            available: false,
            data: None,
            source: FallbackSource::None,
            age_secs: None,
            error: Some(RpcError::NotFound {
                resource: "fallback".into(),
                id: format!(
                    "{}:{}:{}",
                    request.product_id, request.tenant_id, request.cache_key
                ),
            }),
        }
    }
    pub async fn store_response(&self, request: StoreRequest) -> StoreResponse {
        let cache_key =
            Self::build_cache_key(&request.product_id, &request.tenant_id, &request.cache_key);
        debug!(
            product_id = % request.product_id, tenant_id = % request.tenant_id, cache_key
            = % cache_key, ttl_secs = request.ttl_secs, "Storing fallback data"
        );
        let params = SetCacheEntryParams {
            key: cache_key.clone(),
            data: request.data.clone(),
            ttl: Some(std::time::Duration::from_secs(request.ttl_secs)),
        };
        self.cache.set(params).await;
        CbProMetricsRecorder::record_protection_trigger("snapshot_stored");
        info!(
            product_id = % request.product_id, cache_key = % cache_key,
            "Stored fallback data successfully"
        );
        StoreResponse {
            success: true,
            cache_key,
            error: None,
        }
    }
    fn build_cache_key(product_id: &str, tenant_id: &str, key: &str) -> String {
        format!("{}:{}:{}", product_id, tenant_id, key)
    }
}
