pub use crate::common::HealthResponse;
pub use crate::error::RpcError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheEntry {
    pub value: Vec<u8>,
    pub ttl_seconds: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CacheStats {
    pub total_entries: i64,
    pub total_size_bytes: i64,
    pub hit_count: i64,
    pub miss_count: i64,
    pub hit_rate: f64,
    pub eviction_count: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetRequest {
    pub tenant_id: String,
    pub key: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GetResponse {
    pub found: bool,
    #[serde(with = "serde_bytes")]
    pub value: Vec<u8>,
    pub ttl_remaining: i64,
    #[serde(default)]
    pub error: Option<RpcError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetRequest {
    pub tenant_id: String,
    pub key: String,
    #[serde(with = "serde_bytes")]
    pub value: Vec<u8>,
    pub ttl_seconds: i32,
    /// Optional API path used to resolve tenant `api_ttl` when `ttl_seconds` is 0.
    #[serde(default)]
    pub endpoint: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SetResponse {
    pub success: bool,
    #[serde(default)]
    pub error: Option<RpcError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteRequest {
    pub tenant_id: String,
    pub key: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DeleteResponse {
    pub success: bool,
    pub was_found: bool,
    #[serde(default)]
    pub error: Option<RpcError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClearRequest {
    pub tenant_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ClearResponse {
    pub success: bool,
    pub entries_cleared: i32,
    #[serde(default)]
    pub error: Option<RpcError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExistsRequest {
    pub tenant_id: String,
    pub key: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ExistsResponse {
    pub exists: bool,
    #[serde(default)]
    pub error: Option<RpcError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetMultiRequest {
    pub tenant_id: String,
    pub keys: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GetMultiResponse {
    pub values: HashMap<String, Vec<u8>>,
    #[serde(default)]
    pub missing_keys: Vec<String>,
    #[serde(default)]
    pub error: Option<RpcError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetMultiRequest {
    pub tenant_id: String,
    pub entries: HashMap<String, CacheEntry>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SetMultiResponse {
    pub success: bool,
    pub entries_set: i32,
    #[serde(default)]
    pub error: Option<RpcError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetStatsRequest {
    #[serde(default)]
    pub tenant_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GetStatsResponse {
    pub stats: Option<CacheStats>,
    #[serde(default)]
    pub error: Option<RpcError>,
}
#[tarpc::service]
pub trait CacheService {
    async fn get(request: GetRequest) -> GetResponse;
    async fn set(request: SetRequest) -> SetResponse;
    async fn delete(request: DeleteRequest) -> DeleteResponse;
    async fn clear(request: ClearRequest) -> ClearResponse;
    async fn exists(request: ExistsRequest) -> ExistsResponse;
    async fn get_multi(request: GetMultiRequest) -> GetMultiResponse;
    async fn set_multi(request: SetMultiRequest) -> SetMultiResponse;
    async fn get_stats(request: GetStatsRequest) -> GetStatsResponse;
    async fn health() -> HealthResponse;
}
