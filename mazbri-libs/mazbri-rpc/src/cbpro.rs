pub use crate::common::HealthResponse;
pub use crate::error::RpcError;
use serde::{Deserialize, Serialize};
use tarpc::service;
pub const MAX_CACHE_PAYLOAD_SIZE: usize = 10 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FallbackSource {
    MemoryCache,
    DatabaseSnapshot,
    StaleResponse,
    None,
}
impl std::fmt::Display for FallbackSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FallbackSource::MemoryCache => write!(f, "memory_cache"),
            FallbackSource::DatabaseSnapshot => write!(f, "database_snapshot"),
            FallbackSource::StaleResponse => write!(f, "stale_response"),
            FallbackSource::None => write!(f, "none"),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FallbackRequest {
    pub product_id: String,
    pub tenant_id: String,
    pub cache_key: String,
    pub path: String,
    /// Request body snapshot; encoded as JSON text for bincode compatibility.
    #[serde(default, with = "crate::serde_json_value_as_text::option")]
    pub body: Option<serde_json::Value>,
}
impl FallbackRequest {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate_payload_size(&self) -> Result<(), String> {
        if let Some(ref body) = self.body {
            let size = serde_json::to_vec(body).map_or(usize::MAX, |v| v.len());
            if size > MAX_CACHE_PAYLOAD_SIZE {
                return Err(format!(
                    "Cache payload too large: {size} bytes (max {MAX_CACHE_PAYLOAD_SIZE} bytes)"
                ));
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackResponse {
    pub available: bool,
    /// Cached snapshot payload; encoded as JSON text for bincode compatibility.
    #[serde(default, with = "crate::serde_json_value_as_text::option")]
    pub data: Option<serde_json::Value>,
    pub source: FallbackSource,
    #[serde(default)]
    pub age_secs: Option<i64>,
    #[serde(default)]
    pub error: Option<RpcError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreRequest {
    pub product_id: String,
    pub tenant_id: String,
    pub cache_key: String,
    /// Snapshot body; encoded as JSON text for bincode compatibility
    /// (`serde_json::Value` cannot round-trip through bincode's deserializer).
    #[serde(with = "crate::serde_json_value_as_text")]
    pub data: serde_json::Value,
    pub ttl_secs: u64,
    #[serde(default)]
    pub tags: Vec<String>,
}
impl StoreRequest {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate_payload_size(&self) -> Result<(), String> {
        let size = serde_json::to_vec(&self.data).map_or(usize::MAX, |v| v.len());
        if size > MAX_CACHE_PAYLOAD_SIZE {
            return Err(format!(
                "Cache payload too large: {size} bytes (max {MAX_CACHE_PAYLOAD_SIZE} bytes)"
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreResponse {
    pub success: bool,
    pub cache_key: String,
    #[serde(default)]
    pub error: Option<RpcError>,
}
#[service]
pub trait CBProService {
    async fn get_fallback(request: FallbackRequest) -> FallbackResponse;
    async fn store_response(request: StoreRequest) -> StoreResponse;
    async fn health() -> HealthResponse;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::Options;

    #[test]
    fn store_request_roundtrips_through_bincode() {
        let opts = bincode::DefaultOptions::new();
        let req = StoreRequest {
            product_id: "p".into(),
            tenant_id: "t".into(),
            cache_key: "k".into(),
            data: serde_json::json!({"status": 200, "data": {"hello": "world"}}),
            ttl_secs: 60,
            tags: vec!["probe".into()],
        };
        let bytes = opts.serialize(&req).expect("serialize");
        let decoded: StoreRequest = opts.deserialize(&bytes).expect("deserialize");
        assert_eq!(decoded.product_id, "p");
        assert_eq!(decoded.data["status"], 200);
        assert_eq!(decoded.data["data"]["hello"], "world");
    }

    #[test]
    fn fallback_request_with_body_roundtrips_through_bincode() {
        let opts = bincode::DefaultOptions::new();
        let req = FallbackRequest {
            product_id: "p".into(),
            tenant_id: "t".into(),
            cache_key: "k".into(),
            path: "/posts".into(),
            body: Some(serde_json::json!({"page": "1"})),
        };
        let bytes = opts.serialize(&req).expect("serialize");
        let decoded: FallbackRequest = opts.deserialize(&bytes).expect("deserialize");
        assert_eq!(decoded.body.unwrap()["page"], "1");
    }

    #[test]
    fn fallback_response_with_data_roundtrips_through_bincode() {
        let opts = bincode::DefaultOptions::new();
        let resp = FallbackResponse {
            available: true,
            data: Some(serde_json::json!({"status": 200})),
            source: FallbackSource::MemoryCache,
            age_secs: Some(3),
            error: None,
        };
        let bytes = opts.serialize(&resp).expect("serialize");
        let decoded: FallbackResponse = opts.deserialize(&bytes).expect("deserialize");
        assert!(decoded.available);
        assert_eq!(decoded.data.unwrap()["status"], 200);
    }
}
