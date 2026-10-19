pub use crate::common::HealthResponse;
pub use crate::error::RpcError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-API / per-product QPS override carried in policy `api_limits`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ApiLimitRule {
    #[serde(default)]
    pub qps: Option<u64>,
    #[serde(default)]
    pub burst: Option<u64>,
}

/// Tenant rate-limit policy snapshot for gateway local enforcement.
///
/// Fields match what the gateway `LocalRateLimitEngine` actually applies
/// (tenant/API/IP QPS + bandwidth). Dead algorithm / daily / global / shard
/// knobs are intentionally absent.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct PolicySnapshot {
    pub id: String,
    pub tenant_id: String,
    pub version: i64,
    pub enabled: bool,
    pub enforcement_mode: String,
    pub stale_policy: String,
    pub max_snapshot_age_secs: i32,
    pub tenant_qps: i32,
    pub tenant_burst: i32,
    pub client_ip_qps: i32,
    pub client_ip_burst: i32,
    /// Bit per second. `0` disables bandwidth enforcement. UI Mbps = bandwidth_bps / 1_000_000.
    pub bandwidth_bps: i64,
    #[serde(default)]
    pub api_limits: HashMap<String, ApiLimitRule>,
    pub description: Option<String>,
    pub updated_by: Option<String>,
    pub created_at_unix: i64,
    pub updated_at_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GetPolicySnapshotsRequest {
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub min_version: i64,
    #[serde(default)]
    pub include_disabled: bool,
    #[serde(default)]
    pub page_size: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetPolicySnapshotsResponse {
    pub snapshots: Vec<PolicySnapshot>,
    pub server_time_unix: i64,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct UsageDelta {
    pub gateway_id: String,
    pub tenant_id: String,
    pub policy_version: i64,
    #[serde(default)]
    pub product_id: String,
    #[serde(default)]
    pub api_endpoint: String,
    #[serde(default)]
    pub client_ip: Option<String>,
    pub window_start_unix: i64,
    pub window_secs: i32,
    pub allowed_count: i64,
    pub denied_count: i64,
    pub bytes_in: i64,
    pub bytes_out: i64,
    pub snapshot_age_ms: i64,
    pub degraded: bool,
    #[serde(default)]
    pub degrade_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct IngestUsageDeltasRequest {
    #[serde(default)]
    pub deltas: Vec<UsageDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IngestUsageDeltasResponse {
    pub success: bool,
    pub ingested: i64,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ControlCommand {
    pub id: String,
    pub tenant_id: String,
    pub policy_version: i64,
    pub command_type: String,
    #[serde(default)]
    pub command_payload: String,
    pub effective_at_unix: i64,
    #[serde(default)]
    pub expires_at_unix: Option<i64>,
    #[serde(default)]
    pub created_by: Option<String>,
    pub created_at_unix: i64,
    #[serde(default)]
    pub acknowledged_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GetControlCommandsRequest {
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub page_size: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetControlCommandsResponse {
    pub commands: Vec<ControlCommand>,
    pub server_time_unix: i64,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AckControlCommandsRequest {
    #[serde(default)]
    pub gateway_id: String,
    pub command_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AckControlCommandsResponse {
    pub success: bool,
    pub acknowledged: i64,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[tarpc::service]
pub trait RateLimitService {
    async fn get_policy_snapshots(request: GetPolicySnapshotsRequest)
    -> GetPolicySnapshotsResponse;
    async fn ingest_usage_deltas(request: IngestUsageDeltasRequest) -> IngestUsageDeltasResponse;
    async fn get_control_commands(request: GetControlCommandsRequest)
    -> GetControlCommandsResponse;
    async fn ack_control_commands(request: AckControlCommandsRequest)
    -> AckControlCommandsResponse;
    async fn health() -> HealthResponse;
}
