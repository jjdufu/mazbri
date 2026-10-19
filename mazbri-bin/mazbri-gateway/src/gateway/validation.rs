use super::response::ProxyError;
use mazbri_core::crypto::transport::TransportEnvelope;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
pub(crate) const MAX_PATH_LENGTH: usize = 2048;
pub(crate) const MAX_QUERY_PARAMS: usize = 50;
pub(crate) const MAX_HEADERS: usize = 50;
pub(crate) const MAX_HEADER_KEY_LENGTH: usize = 256;
pub(crate) const MAX_HEADER_VALUE_LENGTH: usize = 4096;
pub(crate) const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;
pub(crate) fn sanitize_input(input: &str) -> String {
    const MAX_LENGTH: usize = 100;
    let truncated = if input.len() > MAX_LENGTH {
        let mut end = MAX_LENGTH;
        while !input.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &input[..end]
    } else {
        input
    };
    truncated
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GatewayRequestBody {
    Json(serde_json::Value),
    Raw(String),
    Empty,
}
impl GatewayRequestBody {
    pub fn validate(&self) -> Result<(), ProxyError> {
        match self {
            GatewayRequestBody::Json(val) => {
                if val.is_object() || val.is_array() || val.is_string() {
                    Ok(())
                } else if val.is_null() {
                    Err(ProxyError::BadRequest(
                        "Request body must not be null".to_string(),
                    ))
                } else {
                    Err(ProxyError::BadRequest("Invalid JSON body type".to_string()))
                }
            }
            GatewayRequestBody::Raw(s) => {
                if s.len() > MAX_BODY_SIZE {
                    Err(ProxyError::BadRequest(format!(
                        "Body too large: max {} bytes",
                        MAX_BODY_SIZE
                    )))
                } else {
                    Ok(())
                }
            }
            GatewayRequestBody::Empty => Ok(()),
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayRequest {
    pub product_id: String,
    pub path: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub query: Option<HashMap<String, String>>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub body: Option<GatewayRequestBody>,
}
fn default_method() -> String {
    "POST".to_string()
}
impl GatewayRequest {
    pub fn validate(&self) -> Result<(), ProxyError> {
        if self.product_id.is_empty() {
            return Err(ProxyError::BadRequest("product_id 不能为空".to_string()));
        }
        if self.product_id.len() > 64 {
            return Err(ProxyError::BadRequest(
                "product_id 长度超过限制: 最大 64 字节".to_string(),
            ));
        }
        if !self
            .product_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
        {
            return Err(ProxyError::BadRequest(
                "product_id 包含非法字符，仅允许字母、数字、-、_、.、/".to_string(),
            ));
        }
        if self.path.len() > MAX_PATH_LENGTH {
            return Err(ProxyError::BadRequest(format!(
                "路径长度超过限制: 最大 {} 字节",
                MAX_PATH_LENGTH
            )));
        }
        if let Some(ref query) = self.query {
            if query.len() > MAX_QUERY_PARAMS {
                return Err(ProxyError::BadRequest(format!(
                    "查询参数数量超过限制: 最大 {} 个",
                    MAX_QUERY_PARAMS
                )));
            }
            for (key, value) in query {
                if key.len() > MAX_HEADER_KEY_LENGTH {
                    return Err(ProxyError::BadRequest(format!(
                        "查询参数 key 长度超过限制: 最大 {} 字节",
                        MAX_HEADER_KEY_LENGTH
                    )));
                }
                if value.len() > MAX_HEADER_VALUE_LENGTH {
                    return Err(ProxyError::BadRequest(format!(
                        "查询参数 value 长度超过限制: 最大 {} 字节",
                        MAX_HEADER_VALUE_LENGTH
                    )));
                }
            }
        }
        if let Some(ref headers) = self.headers {
            if headers.len() > MAX_HEADERS {
                return Err(ProxyError::BadRequest(format!(
                    "请求头数量超过限制: 最大 {} 个",
                    MAX_HEADERS
                )));
            }
            for (key, value) in headers {
                if key.len() > MAX_HEADER_KEY_LENGTH {
                    return Err(ProxyError::BadRequest(format!(
                        "请求头 key 长度超过限制: 最大 {} 字节",
                        MAX_HEADER_KEY_LENGTH
                    )));
                }
                if value.len() > MAX_HEADER_VALUE_LENGTH {
                    return Err(ProxyError::BadRequest(format!(
                        "请求头 value 长度超过限制: 最大 {} 字节",
                        MAX_HEADER_VALUE_LENGTH
                    )));
                }
            }
        }
        if let Some(ref body) = self.body {
            body.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum GatewayRequestPayload {
    Plain(GatewayRequest),
    Encrypted(TransportEnvelope),
}
