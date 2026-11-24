use super::execution::ProxyBody;
use super::response::ProxyError;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

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

#[derive(Serialize)]
struct SuccessEnvelope<'a, D: Serialize> {
    success: bool,
    data: D,
    status: u16,
    request_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body_encoding: Option<&'static str>,
}

#[derive(Serialize, Deserialize)]
struct ErrorEnvelope<'a> {
    success: bool,
    error: &'a str,
    status: u16,
    request_id: &'a str,
}

pub(crate) fn serialize_proxy_response(
    status: u16,
    request_id: &str,
    body: &ProxyBody,
    error: Option<&str>,
    content_type: Option<&str>,
) -> Result<SerializedProxyResponse, ProxyError> {
    let success = (200..300).contains(&status) && error.is_none();
    let (body_kind, body_encoding, bytes) = if success {
        serialize_success_response(status, request_id, body, content_type)?
    } else {
        serialize_error_response(status, request_id, error.unwrap_or("上游请求失败"))?
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
        ProxyBody::JsonRawStr(raw) => {
            serde_json::from_str::<serde_json::Value>(raw).unwrap_or_else(|_| {
                serde_json::json!({
                    "raw": raw.as_ref(),
                    "body_encoding": "json_raw_invalid"
                })
            })
        }
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
        ProxyBody::JsonValue(_) | ProxyBody::JsonRaw(_) | ProxyBody::JsonRawStr(_) => "json",
        ProxyBody::Text(_) => "text",
        ProxyBody::Binary(_) => "base64",
    }
}

fn encode_envelope<D: Serialize>(
    status: u16,
    request_id: &str,
    data: D,
    content_type: Option<&str>,
    body_encoding: Option<&'static str>,
) -> Result<Vec<u8>, ProxyError> {
    let envelope = SuccessEnvelope {
        success: true,
        data,
        status,
        request_id,
        content_type,
        body_encoding,
    };
    serde_json::to_vec(&envelope).map_err(|error| {
        ProxyError::InternalError(format!("failed to serialize proxy response: {error}"))
    })
}

fn serialize_success_response(
    status: u16,
    request_id: &str,
    body: &ProxyBody,
    content_type: Option<&str>,
) -> Result<(&'static str, &'static str, Vec<u8>), ProxyError> {
    match body {
        ProxyBody::Empty => {
            let bytes = encode_envelope(status, request_id, serde_json::Value::Null, None, None)?;
            Ok(("empty", "empty", bytes))
        }
        ProxyBody::JsonValue(value) => {
            let bytes = encode_envelope(status, request_id, value, None, None)?;
            Ok(("json_value", "json", bytes))
        }
        ProxyBody::JsonRawStr(raw) => {
            let bytes = if let Ok(rv) = serde_json::from_str::<&RawValue>(raw) {
                encode_envelope(status, request_id, rv, None, None)?
            } else {
                let fallback = serde_json::json!({
                    "raw": raw.as_ref(),
                    "body_encoding": "json_raw_invalid"
                });
                encode_envelope(status, request_id, &fallback, None, None)?
            };
            Ok(("json_raw", "json", bytes))
        }
        ProxyBody::JsonRaw(raw) => {
            let bytes = if let Ok(s) = std::str::from_utf8(raw)
                && let Ok(rv) = serde_json::from_str::<&RawValue>(s)
            {
                encode_envelope(status, request_id, rv, None, None)?
            } else {
                let fallback = serde_json::json!({
                    "raw": String::from_utf8_lossy(raw),
                    "body_encoding": "json_raw_invalid"
                });
                encode_envelope(status, request_id, &fallback, None, None)?
            };
            Ok(("json_raw", "json", bytes))
        }
        ProxyBody::Text(text) => {
            let ct = content_type.unwrap_or("text/plain; charset=utf-8");
            let bytes = encode_envelope(status, request_id, text.as_str(), Some(ct), Some("text"))?;
            Ok(("text", "text", bytes))
        }
        ProxyBody::Binary(bin) => {
            let ct = content_type.unwrap_or("application/octet-stream");
            let encoded = STANDARD.encode(bin);
            let bytes =
                encode_envelope(status, request_id, encoded.as_str(), Some(ct), Some("base64"))?;
            Ok(("binary", "base64", bytes))
        }
    }
}

fn serialize_error_response(
    status: u16,
    request_id: &str,
    error: &str,
) -> Result<(&'static str, &'static str, Vec<u8>), ProxyError> {
    let envelope = ErrorEnvelope {
        success: false,
        error,
        status,
        request_id,
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|err| {
        ProxyError::InternalError(format!("failed to serialize proxy response: {err}"))
    })?;
    Ok(("empty", "json", bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn json_raw_str_splices_exact_payload_without_dom_reordering() {
        let raw = r#"{"z":1,"a":[true,"ok"]}"#;
        let out = serialize_proxy_response(
            200,
            "req-1",
            &ProxyBody::JsonRawStr(Arc::from(raw)),
            None,
            Some("application/json"),
        )
        .unwrap();
        assert!(out.success);
        assert_eq!(out.body_kind, "json_raw");
        assert_eq!(out.body_encoding, "json");
        let text = String::from_utf8(out.bytes).unwrap();
        assert_eq!(
            text,
            r#"{"success":true,"data":{"z":1,"a":[true,"ok"]},"status":200,"request_id":"req-1"}"#
        );
    }

    #[test]
    fn invalid_json_raw_falls_back_cleanly() {
        let out = serialize_proxy_response(
            200,
            "req-2",
            &ProxyBody::JsonRaw(Arc::from(&b"not-json"[..])),
            None,
            None,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out.bytes).unwrap();
        assert_eq!(v["data"]["body_encoding"], "json_raw_invalid");
        assert_eq!(v["data"]["raw"], "not-json");
    }
}
