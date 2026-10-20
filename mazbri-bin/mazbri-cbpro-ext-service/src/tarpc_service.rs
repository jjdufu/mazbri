use crate::manager::CbProExtManager;
use mazbri_core::metrics::CbProMetricsRecorder;
use mazbri_rpc::{
    CBProExtService, HealthResponse, ObservationRequest, ObservationResponse,
    RuntimeDecisionRequest, RuntimeDecisionResponse, ValidationEnqueueRequest,
    ValidationEnqueueResponse, ValidationResultRequest, ValidationResultResponse,
};
use std::sync::Arc;
use tarpc::context::Context;
use tracing::info;

#[derive(Clone)]
pub struct CBProExtServiceImpl {
    manager: Arc<CbProExtManager>,
}

impl CBProExtServiceImpl {
    #[must_use]
    pub fn new(manager: Arc<CbProExtManager>) -> Self {
        Self { manager }
    }
}

impl CBProExtService for CBProExtServiceImpl {
    async fn record_observation(
        self,
        _context: Context,
        request: ObservationRequest,
    ) -> ObservationResponse {
        info!(
            event_type = % request.event_type, request_id = % request.request_id,
            tenant_id = % request.tenant_id, product_id = % request.product_id,
            "record_observation called"
        );
        self.manager.record_observation(request).await
    }

    async fn get_runtime_decision(
        self,
        _context: Context,
        request: RuntimeDecisionRequest,
    ) -> RuntimeDecisionResponse {
        info!(
            tenant_id = % request.tenant_id, product_id = % request.product_id,
            cache_key = % request.cache_key, "get_runtime_decision called"
        );
        self.manager.get_runtime_decision(request).await
    }

    async fn enqueue_validation(
        self,
        _context: Context,
        request: ValidationEnqueueRequest,
    ) -> ValidationEnqueueResponse {
        info!(
            cache_key = % request.cache_key, reason = % request.reason,
            "enqueue_validation called"
        );
        self.manager.enqueue_validation(request).await
    }

    async fn report_validation_result(
        self,
        _context: Context,
        request: ValidationResultRequest,
    ) -> ValidationResultResponse {
        info!(
            cache_key = % request.cache_key, job_id = % request.job_id,
            "report_validation_result called"
        );
        self.manager.report_validation_result(request).await
    }

    fn health(self, _context: Context) -> impl std::future::Future<Output = HealthResponse> {
        CbProMetricsRecorder::set_current_state(1);
        std::future::ready(HealthResponse {
            status: "ok".to_string(),
            service: "cbpro-ext-service".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        })
    }
}
