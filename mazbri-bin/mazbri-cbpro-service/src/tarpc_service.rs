use crate::manager::FallbackManager;
use mazbri_core::metrics::CbProMetricsRecorder;
use mazbri_rpc::{
    CBProService, FallbackRequest, FallbackResponse, HealthResponse, RpcError, StoreRequest,
    StoreResponse,
};
use std::sync::Arc;
use tarpc::context::Context;
use tracing::info;
#[derive(Clone)]
pub struct CBProServiceImpl {
    manager: Arc<FallbackManager>,
}
impl CBProServiceImpl {
    pub fn new(manager: Arc<FallbackManager>) -> Self {
        Self { manager }
    }
}
impl CBProService for CBProServiceImpl {
    async fn get_fallback(self, _context: Context, request: FallbackRequest) -> FallbackResponse {
        info!(
            product_id = % request.product_id, tenant_id = % request.tenant_id, cache_key
            = % request.cache_key, "get_fallback called"
        );
        if let Err(e) = request.validate_payload_size() {
            CbProMetricsRecorder::record_protection_trigger("fallback_payload_rejected");
            return FallbackResponse {
                available: false,
                data: None,
                source: mazbri_rpc::FallbackSource::None,
                age_secs: None,
                error: Some(RpcError::Validation {
                    field: Some("body".into()),
                    message: e,
                }),
            };
        }
        self.manager.get_fallback(request).await
    }
    async fn store_response(self, _context: Context, request: StoreRequest) -> StoreResponse {
        info!(
            product_id = % request.product_id, cache_key = % request.cache_key, ttl_secs
            = request.ttl_secs, "store_response called"
        );
        if let Err(e) = request.validate_payload_size() {
            CbProMetricsRecorder::record_protection_trigger("snapshot_payload_rejected");
            return StoreResponse {
                success: false,
                cache_key: String::new(),
                error: Some(RpcError::Validation {
                    field: Some("data".into()),
                    message: e,
                }),
            };
        }
        self.manager.store_response(request).await
    }
    fn health(self, _context: Context) -> impl std::future::Future<Output = HealthResponse> {
        std::future::ready(HealthResponse {
            status: "ok".to_string(),
            service: "cbpro-service".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        })
    }
}
