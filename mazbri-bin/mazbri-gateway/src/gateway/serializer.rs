use super::execution::ProxyBody;
use super::response::ProxyError;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use mazbri_core::{
    FuryErrorEnvelope, FurySuccessEnvelope, PayloadCodec, serialize_fury_error,
    serialize_fury_success, serialize_payload,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub(crate) struct SerializedProxyResponse {
    pub(crate) status: u16,
    pub(crate) success: bool,
    pub(crate) response_bytes: u64,
    pub(crate) body_kind: &'static str,
    pub(crate) body_encoding: &'static str,
    pub(crate) content_type: Option<String>,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct SuccessEnvelope {
    success: bool,
    data: serde_json::Value,
    status: u16,
    request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body_encoding: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ErrorEnvelope {
    success: bool,
    error: String,
    status: u16,
    request_id: String,
}

pub(crate) fn serialize_proxy_response(
    codec: PayloadCodec,
    status: u16,
    request_id: &str,
    body: &ProxyBody,
    error: Option<&str>,
    content_type: Option<&str>,
) -> Result<SerializedProxyResponse, ProxyError> {
    let success = (200..300).contains(&status) && error.is_none();
    let (body_kind, body_encoding, bytes) = if success {
        serialize_success_response(codec, status, request_id, body, content_type)?
    } else {
        serialize_error_response(codec, status, request_id, error.unwrap_or("上游请求失败"))?
    };
    let effective_content_type = match body {
        ProxyBody::Text(_) => Some(
            content_type
                .unwrap_or("text/plain; charset=utf-8")
                .to_string(),
        ),
        ProxyBody::Binary(_) => Some(
            content_type
                .unwrap_or("application/octet-stream")
                .to_string(),
        ),
        _ => content_type.map(std::string::ToString::to_string),
    };
    let response_bytes = bytes.len() as u64;
    Ok(SerializedProxyResponse {
        status,
        success,
        response_bytes,
        body_kind,
        body_encoding,
        content_type: effective_content_type,
        bytes,
    })
}

pub(crate) fn proxy_body_to_json_value(body: &ProxyBody) -> serde_json::Value {
    match body {
        ProxyBody::Empty => serde_json::Value::Null,
        ProxyBody::JsonValue(value) => value.clone(),
        ProxyBody::JsonRaw(raw) => {
            serde_json::from_slice::<serde_json::Value>(raw).unwrap_or_else(|_| {
                serde_json::json!({
                    "raw": String::from_utf8_lossy(raw),
                    "body_encoding": "json_raw_invalid"
                })
            })
        }
        ProxyBody::Text(text) => serde_json::Value::String(text.clone()),
        ProxyBody::Binary(bytes) => serde_json::Value::String(STANDARD.encode(bytes)),
    }
}

pub(crate) fn proxy_body_encoding(body: &ProxyBody) -> &'static str {
    match body {
        ProxyBody::Empty => "empty",
        ProxyBody::JsonValue(_) | ProxyBody::JsonRaw(_) => "json",
        ProxyBody::Text(_) => "text",
        ProxyBody::Binary(_) => "base64",
    }
}

fn serialize_success_response(
    codec: PayloadCodec,
    status: u16,
    request_id: &str,
    body: &ProxyBody,
    content_type: Option<&str>,
) -> Result<(&'static str, &'static str, Vec<u8>), ProxyError> {
    let (body_kind, body_encoding, bytes) = match codec {
        PayloadCodec::Json => {
            let (data, effective_content_type, envelope_body_encoding) = match body {
                ProxyBody::Empty => (serde_json::Value::Null, None, None),
                ProxyBody::JsonValue(value) => (value.clone(), None, None),
                ProxyBody::JsonRaw(raw) => (
                    serde_json::from_slice::<serde_json::Value>(raw).unwrap_or_else(|_| {
                        serde_json::json!({
                            "raw": String::from_utf8_lossy(raw),
                            "body_encoding": "json_raw_invalid"
                        })
                    }),
                    None,
                    None,
                ),
                ProxyBody::Text(text) => {
                    let content_type = content_type.unwrap_or("text/plain; charset=utf-8");
                    (
                        serde_json::Value::String(text.clone()),
                        Some(content_type.to_string()),
                        Some("text".to_string()),
                    )
                }
                ProxyBody::Binary(bytes) => {
                    let content_type = content_type.unwrap_or("application/octet-stream");
                    (
                        serde_json::Value::String(STANDARD.encode(bytes)),
                        Some(content_type.to_string()),
                        Some("base64".to_string()),
                    )
                }
            };
            let envelope = SuccessEnvelope {
                success: true,
                data,
                status,
                request_id: request_id.to_string(),
                content_type: effective_content_type,
                body_encoding: envelope_body_encoding,
            };
            let bytes = serialize_payload(codec, &envelope).map_err(|error| {
                ProxyError::InternalError(format!("failed to serialize proxy response: {error}"))
            })?;
            let body_kind = match body {
                ProxyBody::Empty => "empty",
                ProxyBody::JsonValue(_) => "json_value",
                ProxyBody::JsonRaw(_) => "json_raw",
                ProxyBody::Text(_) => "text",
                ProxyBody::Binary(_) => "binary",
            };
            let body_encoding = match body {
                ProxyBody::Empty => "empty",
                ProxyBody::JsonValue(_) | ProxyBody::JsonRaw(_) => "json",
                ProxyBody::Text(_) => "text",
                ProxyBody::Binary(_) => "base64",
            };
            (body_kind, body_encoding, bytes)
        }
        PayloadCodec::Fury => {
            let (data, effective_content_type, envelope_body_encoding) =
                fury_success_data(body, content_type)?;
            let envelope = FurySuccessEnvelope {
                success: true,
                data,
                status,
                request_id: request_id.to_string(),
                content_type: effective_content_type,
                body_encoding: envelope_body_encoding,
            };
            let bytes = serialize_fury_success(&envelope).map_err(|error| {
                ProxyError::InternalError(format!("failed to serialize proxy response: {error}"))
            })?;
            let body_kind = match body {
                ProxyBody::Empty => "empty",
                ProxyBody::JsonValue(_) => "json_value",
                ProxyBody::JsonRaw(_) => "json_raw",
                ProxyBody::Text(_) => "text",
                ProxyBody::Binary(_) => "binary",
            };
            let body_encoding = match body {
                ProxyBody::Empty => "empty",
                ProxyBody::JsonValue(_) | ProxyBody::JsonRaw(_) => "json",
                ProxyBody::Text(_) => "text",
                ProxyBody::Binary(_) => "binary",
            };
            (body_kind, body_encoding, bytes)
        }
    };
    Ok((body_kind, body_encoding, bytes))
}

fn serialize_error_response(
    codec: PayloadCodec,
    status: u16,
    request_id: &str,
    error: &str,
) -> Result<(&'static str, &'static str, Vec<u8>), ProxyError> {
    match codec {
        PayloadCodec::Json => {
            let envelope = ErrorEnvelope {
                success: false,
                error: error.to_string(),
                status,
                request_id: request_id.to_string(),
            };
            let bytes = serialize_payload(codec, &envelope).map_err(|err| {
                ProxyError::InternalError(format!("failed to serialize proxy response: {err}"))
            })?;
            Ok(("empty", "json", bytes))
        }
        PayloadCodec::Fury => {
            let envelope = FuryErrorEnvelope {
                success: false,
                error: error.to_string(),
                status,
                request_id: request_id.to_string(),
            };
            let bytes = serialize_fury_error(&envelope).map_err(|err| {
                ProxyError::InternalError(format!("failed to serialize proxy response: {err}"))
            })?;
            Ok(("empty", "json", bytes))
        }
    }
}

type FurySuccessData = (Vec<u8>, Option<String>, Option<String>);

fn fury_success_data(
    body: &ProxyBody,
    content_type: Option<&str>,
) -> Result<FurySuccessData, ProxyError> {
    match body {
        ProxyBody::Empty => Ok((b"null".to_vec(), None, None)),
        ProxyBody::JsonValue(value) => Ok((
            serde_json::to_vec(value).map_err(|error| {
                ProxyError::InternalError(format!(
                    "failed to serialize proxy response data: {error}"
                ))
            })?,
            None,
            None,
        )),
        ProxyBody::JsonRaw(raw) => Ok((raw.to_vec(), None, None)),
        ProxyBody::Text(text) => {
            let content_type = content_type.unwrap_or("text/plain; charset=utf-8");
            Ok((
                text.clone().into_bytes(),
                Some(content_type.to_string()),
                Some("text".to_string()),
            ))
        }
        ProxyBody::Binary(bytes) => {
            let content_type = content_type.unwrap_or("application/octet-stream");
            Ok((
                bytes.clone(),
                Some(content_type.to_string()),
                Some("binary".to_string()),
            ))
        }
    }
}
