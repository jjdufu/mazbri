pub use crate::common::HealthResponse;
pub use crate::error::RpcError;
use serde::{Deserialize, Serialize};
use tarpc::service;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationRequest {
    pub event_type: String,
    pub request_id: String,
    pub tenant_id: String,
    pub product_id: String,
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub cache_key: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub service_total_ms: Option<u64>,
    #[serde(default)]
    pub response_hash: Option<String>,
    #[serde(default)]
    pub response_size_bytes: Option<u64>,
    pub occurred_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObservationResponse {
    pub success: bool,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeDecisionRequest {
    pub tenant_id: String,
    pub product_id: String,
    pub method: String,
    pub path: String,
    pub cache_key: String,
    #[serde(default)]
    pub policy_version: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDecision {
    pub cache_key: String,
    pub tenant_id: String,
    pub product_id: String,
    pub method: String,
    pub path: String,
    pub mode: String,
    pub policy_version: u64,
    pub latency_baseline_ms: u64,
    pub latency_p50_ms: u64,
    pub latency_p75_ms: u64,
    pub latency_p95_ms: u64,
    pub jitter_min_ms: u64,
    pub jitter_max_ms: u64,
    pub max_simulated_delay_ms: u64,
    pub current_ttl_secs: u64,
    pub validate_due: bool,
    pub hot_level: u32,
    #[serde(default)]
    pub last_validation_status: Option<String>,
    pub valid_until_unix_ms: i64,
    pub generated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDecisionResponse {
    pub available: bool,
    #[serde(default)]
    pub decision: Option<RuntimeDecision>,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationEnqueueRequest {
    pub cache_key: String,
    pub tenant_id: String,
    pub product_id: String,
    pub method: String,
    pub path: String,
    pub reason: String,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default)]
    pub policy_version: Option<u64>,
}

fn default_priority() -> i32 {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationEnqueueResponse {
    pub accepted: bool,
    #[serde(default)]
    pub job_id: Option<String>,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationResultRequest {
    pub job_id: String,
    pub cache_key: String,
    pub tenant_id: String,
    pub product_id: String,
    pub method: String,
    pub path: String,
    pub success: bool,
    #[serde(default)]
    pub compare_hash: Option<String>,
    #[serde(default)]
    pub response_status: Option<u16>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationResultResponse {
    pub recorded: bool,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[service]
pub trait CBProExtService {
    async fn record_observation(request: ObservationRequest) -> ObservationResponse;
    async fn get_runtime_decision(request: RuntimeDecisionRequest) -> RuntimeDecisionResponse;
    async fn enqueue_validation(request: ValidationEnqueueRequest) -> ValidationEnqueueResponse;
    async fn report_validation_result(request: ValidationResultRequest)
    -> ValidationResultResponse;
    async fn health() -> HealthResponse;
}
