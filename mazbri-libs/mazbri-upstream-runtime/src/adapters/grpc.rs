use http::Version;
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage};
use prost_types::FileDescriptorSet;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::sync::Arc;
use std::time::Instant;

use super::{AdapterCtx, AdapterOutcome, require_artifact, require_object};
use crate::ssrf::extract_ssrf_error;
use crate::types::{BizError, ExecuteError, InvokeInput};

/// 预编译 `FileDescriptorSet` 二进制为共享 [`DescriptorPool`]。
///
/// # Errors
///
/// 二进制非合法 `FileDescriptorSet` 或描述符池构建失败时返回 [`ExecuteError::ArtifactMissing`]。
pub fn decode_grpc_descriptor_pool(bytes: &[u8]) -> Result<Arc<DescriptorPool>, ExecuteError> {
    let fds = FileDescriptorSet::decode(bytes)
        .map_err(|e| ExecuteError::ArtifactMissing(format!("invalid FileDescriptorSet: {e}")))?;
    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .map_err(|e| ExecuteError::ArtifactMissing(format!("descriptor pool: {e}")))?;
    Ok(Arc::new(pool))
}

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    require_object(&ctx.input.payload)?;
    let spec = &*ctx.input.spec;
    let operation = spec
        .operation
        .as_deref()
        .ok_or_else(|| ExecuteError::SpecInvalid("grpc operation missing".into()))?;
    let method = resolve_method(ctx.input, operation)?;

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
    if let Err(e) = ctx.ssrf.validate_url_syntax_and_literal(&url) {
        return Err(ExecuteError::SsrfBlocked(e));
    }

    let header_map = build_grpc_headers(ctx);
    let (outbound, grpc_status_header, grpc_message) =
        send_grpc_http(ctx, &url, operation, header_map, frame).await?;

    if let Some(gs) = grpc_status_header {
        if gs != "0" {
            return Ok(AdapterOutcome {
                auth_rejection: None,
                payload: serde_json::Value::Null,
                raw_payload_json: None,
                biz_error: Some(BizError {
                    code: grpc_code_name(&gs),
                    message: grpc_message.unwrap_or_else(|| format!("grpc-status {gs}")),
                    details: None,
                }),
                response: outbound,
            });
        }
    } else if ctx.is_reject_status(outbound.status) {
        return Ok(AdapterOutcome::raw(outbound));
    } else if !(200..300).contains(&outbound.status) {
        return Err(ExecuteError::UpstreamHttp(format!(
            "grpc HTTP {}",
            outbound.status
        )));
    }

    let proto = decode_grpc_frame(&outbound.body)?;
    let decoded = DynamicMessage::decode(output_desc, proto)
        .map_err(|e| ExecuteError::AdapterFailed(format!("decode protobuf: {e}")))?;
    let payload = serde_json::to_value(&decoded)
        .map_err(|e| ExecuteError::AdapterFailed(format!("protojson response: {e}")))?;
    Ok(AdapterOutcome {
        auth_rejection: None,
        payload,
        raw_payload_json: None,
        biz_error: None,
        response: outbound,
    })
}

async fn send_grpc_http(
    ctx: &AdapterCtx<'_>,
    url: &str,
    operation: &str,
    header_map: HeaderMap,
    frame: Vec<u8>,
) -> Result<(crate::types::OutboundResponse, Option<String>, Option<String>), ExecuteError> {
    let http_client = if url.starts_with("http://") {
        ctx.h2c_client
    } else {
        ctx.client
    };
    let started = Instant::now();
    let resp = match http_client
        .post(url)
        .version(Version::HTTP_2)
        .headers(header_map)
        .timeout(ctx.timeout)
        .body(frame)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            if let Some(ssrf_msg) = extract_ssrf_error(&e) {
                return Err(ExecuteError::SsrfBlocked(ssrf_msg));
            }
            ctx.wire_clock.add(started.elapsed());
            return Err(ExecuteError::UpstreamTransport(format!("grpc: {e}")));
        }
    };
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
    let mut resp_headers = std::collections::HashMap::with_capacity(resp.headers().len());
    for (k, v) in resp.headers() {
        if let Ok(s) = v.to_str() {
            resp_headers.insert(k.as_str().to_string(), s.to_string());
        }
    }
    let body = resp.bytes().await.unwrap_or_default().to_vec();
    let wire_duration = started.elapsed();
    ctx.wire_clock.add(wire_duration);
    Ok((
        crate::types::OutboundResponse {
            status,
            headers: resp_headers,
            body,
            final_url: url.to_string(),
            method: "POST".into(),
            path: format!("/{operation}"),
            protocol: "HTTP/2".into(),
            wire_duration,
        },
        grpc_status_header,
        grpc_message,
    ))
}

fn resolve_method(
    input: &InvokeInput,
    operation: &str,
) -> Result<prost_reflect::MethodDescriptor, ExecuteError> {
    let owned_pool;
    let pool: &DescriptorPool = if let Some(ref cached) = input.descriptor_pool {
        cached.as_ref()
    } else {
        owned_pool = decode_grpc_descriptor_pool(require_artifact(input)?)?;
        owned_pool.as_ref()
    };
    let (service_name, method_name) = operation.rsplit_once('/').ok_or_else(|| {
        ExecuteError::SpecInvalid("grpc operation must be package.Service/Rpc".into())
    })?;
    let service = pool
        .get_service_by_name(service_name)
        .ok_or_else(|| ExecuteError::SpecInvalid(format!("unknown grpc service {service_name}")))?;
    service
        .methods()
        .find(|m| m.name() == method_name)
        .ok_or_else(|| ExecuteError::SpecInvalid(format!("unknown grpc method {method_name}")))
}

fn build_grpc_headers(ctx: &AdapterCtx<'_>) -> HeaderMap {
    let spec = &*ctx.input.spec;
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
        ) && !header_map.contains_key(&hn)
        {
            header_map.insert(hn, hv);
        }
    }
    crate::http_call::inject_correlation_header_map(&mut header_map, &ctx.input.request_id);
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
    header_map
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
        crate::http_call::inject_correlation_header_map(&mut header_map, "rid-grpc");
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
        crate::http_call::inject_correlation_header_map(&mut header_map, "new");
        assert_eq!(
            header_map.get("x-request-id").and_then(|v| v.to_str().ok()),
            Some("kept")
        );
    }

    #[test]
    fn grpc_skips_empty_request_id() {
        let mut header_map = metadata_to_headers(&HashMap::new());
        crate::http_call::inject_correlation_header_map(&mut header_map, "");
        assert!(!header_map.contains_key("x-request-id"));
    }

    #[tokio::test]
    async fn grpc_h2c_blocks_private_host_when_ssrf_disallows_private() {
        use prost_types::{DescriptorProto, FileDescriptorProto, MethodDescriptorProto, ServiceDescriptorProto};
        use std::time::Duration;

        let fds = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("ping.proto".into()),
                package: Some("demo".into()),
                message_type: vec![DescriptorProto {
                    name: Some("Ping".into()),
                    ..Default::default()
                }],
                service: vec![ServiceDescriptorProto {
                    name: Some("Echo".into()),
                    method: vec![MethodDescriptorProto {
                        name: Some("Send".into()),
                        input_type: Some(".demo.Ping".into()),
                        output_type: Some(".demo.Ping".into()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };
        let mut bytes = Vec::new();
        fds.encode(&mut bytes).unwrap();
        let pool = decode_grpc_descriptor_pool(&bytes).unwrap();

        let spec = mazbri_core::ChannelEgressSpec {
            adapter: "grpc".into(),
            base_url: "http://localhost:9999".into(),
            operation: Some("demo.Echo/Send".into()),
            artifact_id: Some("00000000-0000-0000-0000-000000000001".into()),
            ..Default::default()
        };

        let ssrf = crate::ssrf::SsrfChecker::new();
        let client = Arc::new(reqwest::Client::builder().no_proxy().build().unwrap());
        let engine = crate::engine::RuntimeEngine::new(client, ssrf);
        let res = engine
            .invoke(InvokeInput {
                supplier_id: "s".into(),
                supplier_code: "S".into(),
                channel_id: "c".into(),
                request_id: "r".into(),
                payload: serde_json::json!({}),
                spec: Arc::new(spec),
                spec_validated: true,
                config: None,
                timeout: Duration::from_secs(2),
                mode: crate::types::ExecuteMode::Production,
                options: crate::types::ExecuteOptions::default(),
                credentials: HashMap::new(),
                pipeline_keys: None,
                artifact_bytes: Some(Arc::from(bytes)),
                descriptor_pool: Some(pool),
                cache_key: None,
            })
            .await;
        assert!(!res.ok);
        assert_eq!(res.error_code.as_deref(), Some("SSRF_BLOCKED"));
    }
}
