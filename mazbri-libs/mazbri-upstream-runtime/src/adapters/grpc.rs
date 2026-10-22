use http::Version;
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage};
use prost_types::FileDescriptorSet;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::sync::OnceLock;
use std::time::Duration;

use super::{AdapterCtx, AdapterOutcome, require_artifact, require_object};
use crate::types::{BizError, ExecuteError};

fn plaintext_h2_client() -> Result<&'static reqwest::Client, ExecuteError> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(c) = CLIENT.get() {
        return Ok(c);
    }
    let built = reqwest::Client::builder()
        .http2_prior_knowledge()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ExecuteError::AdapterFailed(format!("h2c client: {e}")))?;
    Ok(CLIENT.get_or_init(|| built))
}

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    require_object(&ctx.input.payload)?;
    let spec = &ctx.input.spec;
    let operation = spec
        .operation
        .as_deref()
        .ok_or_else(|| ExecuteError::SpecInvalid("grpc operation missing".into()))?;
    let bytes = require_artifact(ctx.input)?;
    let fds = FileDescriptorSet::decode(bytes)
        .map_err(|e| ExecuteError::ArtifactMissing(format!("invalid FileDescriptorSet: {e}")))?;
    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .map_err(|e| ExecuteError::ArtifactMissing(format!("descriptor pool: {e}")))?;
    let (service_name, method_name) = operation.rsplit_once('/').ok_or_else(|| {
        ExecuteError::SpecInvalid("grpc operation must be package.Service/Rpc".into())
    })?;
    let service = pool
        .get_service_by_name(service_name)
        .ok_or_else(|| ExecuteError::SpecInvalid(format!("unknown grpc service {service_name}")))?;
    let method = service
        .methods()
        .find(|m| m.name() == method_name)
        .ok_or_else(|| ExecuteError::SpecInvalid(format!("unknown grpc method {method_name}")))?;

    let input_desc = method.input();
    let output_desc = method.output();
    let dyn_msg = DynamicMessage::deserialize(input_desc, ctx.input.payload.clone())
        .map_err(|e| ExecuteError::SpecInvalid(format!("protojson request: {e}")))?;
    let mut proto_buf = Vec::new();
    dyn_msg
        .encode(&mut proto_buf)
        .map_err(|e| ExecuteError::AdapterFailed(format!("encode protobuf: {e}")))?;
    let frame = grpc_frame(&proto_buf)?;

    let url = format!(
        "{}/{}",
        ctx.base_url.trim_end_matches('/'),
        operation.trim_start_matches('/')
    );
    if let Err(e) = ctx.ssrf.validate_url(&url).await {
        return Err(ExecuteError::SsrfBlocked(e));
    }

    let mut header_map = HeaderMap::new();
    for (k, v) in &ctx.auth.metadata {
        if let (Ok(hn), Ok(hv)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            header_map.insert(hn, hv);
        }
    }
    for (k, v) in super::merge_request_headers(ctx.auth, spec) {
        if let (Ok(hn), Ok(hv)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(&v),
        ) {
            if !header_map.contains_key(&hn) {
                header_map.insert(hn, hv);
            }
        }
    }
    crate::http_call::inject_x_request_id_header_map(&mut header_map, &ctx.input.request_id);
    header_map.insert(
        HeaderName::from_static("content-type"),
        HeaderValue::from_static("application/grpc"),
    );
    header_map.insert(
        HeaderName::from_static("te"),
        HeaderValue::from_static("trailers"),
    );
    header_map.insert(
        HeaderName::from_static("grpc-encoding"),
        HeaderValue::from_static("identity"),
    );

    let http_client = if url.starts_with("http://") {
        plaintext_h2_client()?
    } else {
        ctx.client
    };
    let resp = http_client
        .post(&url)
        .version(Version::HTTP_2)
        .headers(header_map)
        .timeout(ctx.timeout)
        .body(frame)
        .send()
        .await
        .map_err(|e| ExecuteError::UpstreamTransport(format!("grpc: {e}")))?;

    let status = resp.status().as_u16();
    let grpc_status_header = resp
        .headers()
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let grpc_message = resp
        .headers()
        .get("grpc-message")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let mut resp_headers = std::collections::HashMap::new();
    for (k, v) in resp.headers() {
        if let Ok(s) = v.to_str() {
            resp_headers.insert(k.to_string(), s.to_string());
        }
    }
    let body = resp.bytes().await.unwrap_or_default();
    let outbound = crate::types::OutboundResponse {
        status,
        headers: resp_headers,
        body: body.to_vec(),
        final_url: url.clone(),
        method: "POST".into(),
        path: format!("/{operation}"),
    };

    if let Some(gs) = grpc_status_header {
        if gs != "0" {
            return Ok(AdapterOutcome {
                payload: serde_json::Value::Null,
                biz_error: Some(BizError {
                    code: grpc_code_name(&gs),
                    message: grpc_message.unwrap_or_else(|| format!("grpc-status {gs}")),
                    details: None,
                }),
                response: outbound,
            });
        }
    } else if status == 401 {
        return Ok(AdapterOutcome {
            payload: serde_json::Value::Null,
            biz_error: None,
            response: outbound,
        });
    } else if !(200..300).contains(&status) {
        return Err(ExecuteError::UpstreamHttp(format!("grpc HTTP {status}")));
    }

    let proto = decode_grpc_frame(&body)?;
    let decoded = DynamicMessage::decode(output_desc, proto)
        .map_err(|e| ExecuteError::AdapterFailed(format!("decode protobuf: {e}")))?;
    let payload = serde_json::to_value(&decoded)
        .map_err(|e| ExecuteError::AdapterFailed(format!("protojson response: {e}")))?;
    Ok(AdapterOutcome {
        payload,
        biz_error: None,
        response: outbound,
    })
}

fn grpc_frame(msg: &[u8]) -> Result<Vec<u8>, ExecuteError> {
    let len = u32::try_from(msg.len()).map_err(|_| {
        ExecuteError::AdapterFailed("grpc request message exceeds 4 GiB".to_string())
    })?;
    let mut out = Vec::with_capacity(5 + msg.len());
    out.push(0);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(msg);
    Ok(out)
}

fn decode_grpc_frame(buf: &[u8]) -> Result<&[u8], ExecuteError> {
    if buf.len() < 5 {
        return Err(ExecuteError::AdapterFailed("grpc frame too short".into()));
    }
    if buf[0] != 0 {
        return Err(ExecuteError::AdapterFailed(
            "compressed grpc frames are not supported".into(),
        ));
    }
    let len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
    buf.get(5..5 + len)
        .ok_or_else(|| ExecuteError::AdapterFailed("grpc frame truncated".into()))
}

fn grpc_code_name(code: &str) -> String {
    match code {
        "16" => "UNAUTHORIZED".into(),
        "7" => "FORBIDDEN".into(),
        "5" => "NOT_FOUND".into(),
        "3" => "INVALID_PARAMETER".into(),
        _ => "UPSTREAM_BIZ".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn metadata_to_headers(metadata: &HashMap<String, String>) -> HeaderMap {
        let mut header_map = HeaderMap::new();
        for (k, v) in metadata {
            if let (Ok(hn), Ok(hv)) = (
                HeaderName::from_bytes(k.as_bytes()),
                HeaderValue::from_str(v),
            ) {
                header_map.insert(hn, hv);
            }
        }
        header_map
    }

    #[test]
    fn grpc_injects_x_request_id_when_metadata_missing() {
        let mut header_map = metadata_to_headers(&HashMap::new());
        crate::http_call::inject_x_request_id_header_map(&mut header_map, "rid-grpc");
        assert_eq!(
            header_map.get("x-request-id").and_then(|v| v.to_str().ok()),
            Some("rid-grpc")
        );
    }

    #[test]
    fn grpc_does_not_overwrite_existing_metadata() {
        let mut meta = HashMap::new();
        meta.insert("X-Request-Id".into(), "kept".into());
        let mut header_map = metadata_to_headers(&meta);
        crate::http_call::inject_x_request_id_header_map(&mut header_map, "new");
        assert_eq!(
            header_map.get("x-request-id").and_then(|v| v.to_str().ok()),
            Some("kept")
        );
    }

    #[test]
    fn grpc_skips_empty_request_id() {
        let mut header_map = metadata_to_headers(&HashMap::new());
        crate::http_call::inject_x_request_id_header_map(&mut header_map, "");
        assert!(!header_map.contains_key("x-request-id"));
    }
}
