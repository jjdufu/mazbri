use crate::code::{BillingCode, BillingInfo, ServiceResponse};
pub use crate::common::HealthResponse;
pub use crate::error::RpcError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamBody {
    Json(#[serde(with = "crate::serde_json_value_as_text")] serde_json::Value),
    Raw(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinaryEncoding {
    Raw,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum UpstreamResponseBody {
    #[default]
    Empty,
    JsonValue(#[serde(with = "crate::serde_json_value_as_text")] serde_json::Value),
    JsonRaw(#[serde(with = "serde_bytes")] Vec<u8>),
    Text(String),
    Binary {
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
        encoding: BinaryEncoding,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct HttpUpstreamData {
    pub status: u16,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub body: UpstreamResponseBody,
    #[serde(default)]
    pub upstream_url: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub body_bytes_len: u64,
}

pub type HttpUpstreamResponse = ServiceResponse<HttpUpstreamData>;

impl HttpUpstreamResponse {
    #[must_use]
    pub fn ok(data: HttpUpstreamData) -> Self {
        let code = match data.status {
            200..=299 => BillingCode::SUCCESS,
            404 => BillingCode::NOT_FOUND,
            408 => BillingCode::UPSTREAM_TIMEOUT,
            429 => BillingCode::RATE_LIMIT_EXCEEDED,
            500..=599 => BillingCode::UPSTREAM_ERROR,
            _ => BillingCode::SUCCESS_WITH_DATA,
        };
        Self::success(data, BillingInfo::new(code).with_units(1))
    }

    #[must_use]
    pub fn invalid_parameter(message: impl Into<String>) -> Self {
        Self::error(BillingCode::INVALID_PARAMETER, message)
    }

    #[must_use]
    pub fn upstream_error(message: impl Into<String>) -> Self {
        Self::error(BillingCode::UPSTREAM_ERROR, message)
    }

    #[must_use]
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::error(BillingCode::UPSTREAM_TIMEOUT, message)
    }

    #[must_use]
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::error(BillingCode::INTERNAL_ERROR, message)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpUpstreamRequest {
    pub product_id: String,
    pub tenant_id: String,
    pub path: String,
    pub method: String,
    #[serde(default)]
    pub query: HashMap<String, String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<UpstreamBody>,
    #[serde(default)]
    pub timeout_secs: u64,
}

impl HttpUpstreamRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.product_id.trim().is_empty() {
            return Err("product_id 不能为空".to_string());
        }
        if self.tenant_id.trim().is_empty() {
            return Err("tenant_id 不能为空".to_string());
        }
        if self.path.trim().is_empty() {
            return Err("path 不能为空".to_string());
        }
        if !self.path.starts_with('/') {
            return Err("path 必须以 / 开头".to_string());
        }
        if self.method.trim().is_empty() {
            return Err("method 不能为空".to_string());
        }
        Ok(())
    }
}

#[tarpc::service]
pub trait HttpUpstreamService {
    async fn execute(request: HttpUpstreamRequest) -> HttpUpstreamResponse;
    async fn health() -> HealthResponse;
}
