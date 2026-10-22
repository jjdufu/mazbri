pub use crate::common::HealthResponse;
pub use crate::error::RpcError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TenantContext {
    pub tenant_id: String,
    pub tenant_name: String,
    pub enabled: bool,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    pub created_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ResourceQuota {
    pub max_requests_per_day: i64,
    pub max_storage_bytes: i64,
    pub max_bandwidth_bytes: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateTokenRequest {
    pub api_key: String,
    #[serde(default)]
    pub jwt_token: String,
    #[serde(default)]
    pub client_ip: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ValidateTokenResponse {
    pub valid: bool,
    pub tenant: Option<TenantContext>,
    #[serde(default)]
    pub error: Option<RpcError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetTenantInfoRequest {
    pub tenant_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GetTenantInfoResponse {
    pub tenant: Option<TenantContext>,
    /// Reserved (H-15). Currently always None; use `rate_limit` policies for limits.
    pub quota: Option<ResourceQuota>,
    #[serde(default)]
    pub error: Option<RpcError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateApiKeyRequest {
    pub api_key: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ValidateApiKeyResponse {
    pub valid: bool,
    #[serde(default)]
    pub tenant_id: String,
    #[serde(default)]
    pub api_key_prefix: String,
    /// Scopes configured on the API key (e.g. `read`, `write`, `admin` or `api:*`).
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub error: Option<RpcError>,
}
#[tarpc::service]
pub trait AuthService {
    async fn validate_token(request: ValidateTokenRequest) -> ValidateTokenResponse;
    async fn get_tenant_info(request: GetTenantInfoRequest) -> GetTenantInfoResponse;
    async fn validate_api_key(request: ValidateApiKeyRequest) -> ValidateApiKeyResponse;
    async fn health() -> HealthResponse;
}
