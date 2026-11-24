use super::response::ProxyError;
use mazbri_core::crypto::transport::TransportEnvelope;
use serde::de::Deserializer;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::HashMap;
use std::sync::Arc;

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

/// 网关请求业务载荷：反序列化时直接校验并捕获原始 JSON 文本切片，不构建 `serde_json::Value` 树。
#[derive(Debug, Clone)]
pub struct GatewayRequestBody(Arc<str>);

impl Serialize for GatewayRequestBody {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match serde_json::from_str::<&RawValue>(&self.0) {
            Ok(rv) => rv.serialize(serializer),
            Err(_) => self.0.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for GatewayRequestBody {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        Ok(Self(Arc::from(raw.get())))
    }
}

impl GatewayRequestBody {
    #[must_use]
    pub fn is_json_object(&self) -> bool {
        self.0.trim_start().starts_with('{')
    }

    #[must_use]
    pub fn as_raw_json_str(&self) -> &str {
        self.0.as_ref()
    }

    /// # Errors
    ///
    /// 请求体超过大小上限、为 `null` 或顶层为标量数值/布尔值时返回 `ProxyError::BadRequest`。
    pub fn validate(&self) -> Result<(), ProxyError> {
        let trimmed = self.0.trim_start();
        if trimmed.len() > MAX_BODY_SIZE {
            return Err(ProxyError::BadRequest(format!(
                "Body too large: max {MAX_BODY_SIZE} bytes"
            )));
        }
        match trimmed.as_bytes().first().copied() {
            Some(b'{' | b'[' | b'"') => Ok(()),
            Some(b'n') if trimmed.trim_end() == "null" => Err(ProxyError::BadRequest(
                "Request body must not be null".to_string(),
            )),
            _ => Err(ProxyError::BadRequest("Invalid JSON body type".to_string())),
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
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
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
                "路径长度超过限制: 最大 {MAX_PATH_LENGTH} 字节"
            )));
        }
        if let Some(ref query) = self.query {
            if query.len() > MAX_QUERY_PARAMS {
                return Err(ProxyError::BadRequest(format!(
                    "查询参数数量超过限制: 最大 {MAX_QUERY_PARAMS} 个"
                )));
            }
            for (key, value) in query {
                if key.len() > MAX_HEADER_KEY_LENGTH {
                    return Err(ProxyError::BadRequest(format!(
                        "查询参数 key 长度超过限制: 最大 {MAX_HEADER_KEY_LENGTH} 字节"
                    )));
                }
                if value.len() > MAX_HEADER_VALUE_LENGTH {
                    return Err(ProxyError::BadRequest(format!(
                        "查询参数 value 长度超过限制: 最大 {MAX_HEADER_VALUE_LENGTH} 字节"
                    )));
                }
            }
        }
        if let Some(ref headers) = self.headers {
            if headers.len() > MAX_HEADERS {
                return Err(ProxyError::BadRequest(format!(
                    "请求头数量超过限制: 最大 {MAX_HEADERS} 个"
                )));
            }
            for (key, value) in headers {
                if key.len() > MAX_HEADER_KEY_LENGTH {
                    return Err(ProxyError::BadRequest(format!(
                        "请求头 key 长度超过限制: 最大 {MAX_HEADER_KEY_LENGTH} 字节"
                    )));
                }
                if value.len() > MAX_HEADER_VALUE_LENGTH {
                    return Err(ProxyError::BadRequest(format!(
                        "请求头 value 长度超过限制: 最大 {MAX_HEADER_VALUE_LENGTH} 字节"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_body_accepts_object_array_string_and_rejects_scalars() {
        let req_obj: GatewayRequest =
            serde_json::from_str(r#"{"product_id":"p1","path":"/v1","body":{"k":1,"nested":[true]}}"#)
                .unwrap();
        assert!(req_obj.validate().is_ok());
        assert!(req_obj.body.as_ref().unwrap().is_json_object());
        assert_eq!(
            req_obj.body.as_ref().unwrap().as_raw_json_str(),
            r#"{"k":1,"nested":[true]}"#
        );

        let req_arr: GatewayRequest =
            serde_json::from_str(r#"{"product_id":"p1","path":"/v1","body":[1,2]}"#).unwrap();
        assert!(req_arr.validate().is_ok());
        assert!(!req_arr.body.as_ref().unwrap().is_json_object());

        let req_str: GatewayRequest =
            serde_json::from_str(r#"{"product_id":"p1","path":"/v1","body":"hello"}"#).unwrap();
        assert!(req_str.validate().is_ok());

        let req_num: GatewayRequest =
            serde_json::from_str(r#"{"product_id":"p1","path":"/v1","body":42}"#).unwrap();
        assert!(req_num.validate().is_err());

        let null_body: GatewayRequestBody = serde_json::from_str("null").unwrap();
        assert!(
            null_body
                .validate()
                .unwrap_err()
                .to_string()
                .contains("must not be null")
        );
    }
}
