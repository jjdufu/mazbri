use super::{
    Arc, BYPASS_CONTRACT_ID, Duration, GATEWAY_UPSTREAM_CONTRACT_ERRORS_TOTAL,
    GatewayCandidateChannel, GatewayRequest, GatewayRequestBody, HashMap, Instant, MazbriClaims,
    PreparedChannel, ProxyBody, ProxyState, RetryReason, RngExt, TraceContext, UpstreamResponse,
    record_internal_retries, record_retry, record_timeout_if_any, upstream_proto,
};
use crate::http2_upstream_client::PooledChannel;
use mazbri_core::num::SaturatingFrom;
use serde_json::value::RawValue;

pub(super) async fn execute_http_upstream_request(
    state: &ProxyState,
    req: &GatewayRequest,
    channel: &GatewayCandidateChannel,
    claims: &MazbriClaims,
    attempt: AttemptCtx<'_>,
    trace_context: &TraceContext,
) -> Result<UpstreamResponse, String> {
    match &channel.prepared {
        PreparedChannel::Invalid {
            error,
            unknown_contract_id,
            ..
        } => {
            if let Some(cid) = unknown_contract_id {
                GATEWAY_UPSTREAM_CONTRACT_ERRORS_TOTAL
                    .with_label_values(&[cid.as_ref(), "unknown_contract"])
                    .inc();
            }
            Err(error.to_string())
        }
        PreparedChannel::Transparent {
            adapter,
            timeout_ms,
            supplier_json,
            supplier_hash,
            egress_json,
            egress_hash,
            supplier_sync_key,
            egress_sync_key,
            ..
        } => {
            let span_id = current_attempt_span_id(trace_context);
            let request_id = client_or_trace_request_id(req, trace_context);
            let payload_json = extract_transparent_payload_json(req.body.as_ref())?;
            let bypass_endpoint = state.services_config.bypass.as_ref().ok_or_else(|| {
                "Bypass service endpoint is not configured in ServicesConfig (transparent upstream requires bypass_url)"
                    .to_string()
            })?;
            let bypass_addr = format!("https://{}", bypass_endpoint.url());
            let target = TransparentInvokeSpec {
                req,
                channel,
                claims,
                trace_context,
                request_id: &request_id,
                span_id: &span_id,
                payload_json,
                timeout_ms: *timeout_ms,
                attempt_order: attempt.attempt_order,
                adapter,
                supplier_json,
                supplier_hash,
                egress_json,
                egress_hash,
                supplier_sync_key,
                egress_sync_key,
            };
            invoke_bypass(state, &bypass_addr, &target, attempt).await
        }
        PreparedChannel::Microservice {
            contract_id,
            timeout_secs,
            ..
        } => {
            let headers = build_microservice_headers(req, trace_context, contract_id);
            let request = upstream_proto::HttpUpstreamRequest {
                product_id: req.product_id.clone(),
                tenant_id: claims.tenant_id.clone(),
                path: req.path.clone(),
                method: req.method.clone(),
                query: req.query.clone().unwrap_or_default(),
                headers,
                body: req.body.as_ref().map(proxy_body_to_upstream_body_proto),
                timeout_secs: *timeout_secs,
            };
            execute_plain_request(
                state,
                contract_id,
                channel.supplier_upstream_url.as_str(),
                request,
                attempt,
            )
            .await
        }
    }
}

/// 单通道尝试上下文：重试上限 + 指标 `mode` 标签。
#[derive(Debug, Clone, Copy)]
pub(super) struct AttemptCtx<'a> {
    pub max_retries: u32,
    pub mode: &'a str,
    /// 本请求内第几个候选渠道（从 1 起），透传给 bypass 访问日志。
    pub attempt_order: u32,
}

struct TransparentInvokeSpec<'a> {
    req: &'a GatewayRequest,
    channel: &'a GatewayCandidateChannel,
    claims: &'a MazbriClaims,
    trace_context: &'a TraceContext,
    request_id: &'a str,
    span_id: &'a str,
    payload_json: &'a str,
    timeout_ms: u64,
    attempt_order: u32,
    adapter: &'a Arc<str>,
    supplier_json: &'a Arc<str>,
    supplier_hash: &'a Arc<str>,
    egress_json: &'a Arc<str>,
    egress_hash: &'a Arc<str>,
    supplier_sync_key: &'a Arc<str>,
    egress_sync_key: &'a Arc<str>,
}

impl TransparentInvokeSpec<'_> {
    fn build_request_for_slot(
        &self,
        slot: &PooledChannel,
        force_full: bool,
    ) -> upstream_proto::InvokeRequest {
        let _ = self.adapter;
        let send_egress_full = force_full || !slot.is_synced(self.egress_sync_key, self.egress_hash);
        let send_supplier_full =
            force_full || !slot.is_synced(self.supplier_sync_key, self.supplier_hash);
        upstream_proto::InvokeRequest {
            product_id: self.req.product_id.clone(),
            tenant_id: self.claims.tenant_id.clone(),
            channel_id: self.channel.channel_id.clone(),
            request_id: self.request_id.to_string(),
            trace: Some(upstream_proto::TraceContext {
                request_id: self.trace_context.request_id.to_string(),
                trace_id: self.trace_context.trace_id.to_string(),
                span_id: self.span_id.to_string(),
                parent_span_id: self.trace_context.parent_span_id.to_string(),
                sampled: self.trace_context.sampled,
            }),
            timeout_ms: self.timeout_ms,
            payload_json: self.payload_json.to_string(),
            egress_spec_hash: self.egress_hash.to_string(),
            egress_spec_json: if send_egress_full {
                self.egress_json.to_string()
            } else {
                String::new()
            },
            supplier_id: self.channel.supplier_id.clone(),
            supplier_code: self.channel.supplier_code.clone(),
            supplier_config_hash: self.supplier_hash.to_string(),
            supplier_config_json: if send_supplier_full {
                self.supplier_json.to_string()
            } else {
                String::new()
            },
            attempt_order: self.attempt_order,
            tenant_code: self.claims.tenant_code.clone(),
            channel_code: self.channel.channel_code.clone(),
        }
    }
}

fn current_attempt_span_id(trace_context: &TraceContext) -> String {
    mazbri_core::trace_propagation::current_span_context()
        .map_or_else(|| trace_context.span_id.to_string(), |sc| sc.span_id().to_string())
}

fn client_or_trace_request_id(req: &GatewayRequest, trace_context: &TraceContext) -> String {
    req.headers
        .as_ref()
        .and_then(|h| {
            h.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("x-request-id"))
                .map(|(_, v)| v.clone())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| trace_context.request_id.to_string())
}

fn extract_transparent_payload_json(body: Option<&GatewayRequestBody>) -> Result<&str, String> {
    match body {
        Some(b) if b.is_json_object() => Ok(b.as_raw_json_str()),
        None => Ok("{}"),
        Some(_) => Err(
            "transparent 免代码路径只接受 JSON 对象 body（XML/form/protobuf 由 bypass 组包）"
                .into(),
        ),
    }
}

fn build_microservice_headers(
    req: &GatewayRequest,
    trace_context: &TraceContext,
    contract_id: &str,
) -> HashMap<String, String> {
    let mut headers = sanitize_client_ip_headers(req.headers.clone().unwrap_or_default());
    trace_context.write_propagate_headers(&mut headers);
    if let Some(span_context) = mazbri_core::trace_propagation::current_span_context() {
        headers.insert(
            "traceparent".to_string(),
            mazbri_core::trace_spans::traceparent_of(&span_context),
        );
        headers.insert("x-span-id".to_string(), span_context.span_id().to_string());
    }
    headers.insert(
        "x-mazbri-upstream-mode".to_string(),
        "microservice".to_string(),
    );
    headers.insert("x-mazbri-contract-id".to_string(), contract_id.to_string());
    headers
}

/// 指数退避 + 抖动；返回 `false` 表示重试次数已用尽。
async fn backoff_before_retry(attempt: &mut u32, max_retries: u32) -> bool {
    if *attempt >= max_retries {
        return false;
    }
    let cap = 100u64 * 2u64.pow(*attempt);
    let delay_ms = rand::rng().random_range(0..=cap);
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    *attempt += 1;
    true
}

async fn invoke_bypass(
    state: &ProxyState,
    bypass_addr: &str,
    target: &TransparentInvokeSpec<'_>,
    ctx: AttemptCtx<'_>,
) -> Result<UpstreamResponse, String> {
    let mut attempt = 0u32;
    let mut forced_full_config = false;
    let mut slot = state
        .upstream_http2_pool
        .pick_slot(BYPASS_CONTRACT_ID, bypass_addr)
        .await
        .map_err(|e| format!("upstream HTTP/2 request failed: {e}"))?;

    loop {
        // 正常单次路径直接移动 `InvokeRequest`，零多余克隆。
        let request = target.build_request_for_slot(&slot, forced_full_config);
        let response = state
            .upstream_http2_pool
            .execute_invoke_on_slot(&slot, BYPASS_CONTRACT_ID, bypass_addr, request)
            .await;
        if let Ok(ref response) = response {
            record_internal_retries(ctx.mode, response.internal_retries, &response.retry_reasons);
        }
        match response {
            Ok(response) if !response.success => {
                let error = if response.error.is_empty() {
                    "upstream request failed".to_string()
                } else {
                    response.error
                };
                if !forced_full_config && error.contains("CONFIG_CACHE_MISS") {
                    slot.invalidate_key(target.egress_sync_key);
                    slot.invalidate_key(target.supplier_sync_key);
                    forced_full_config = true;
                    record_retry(ctx.mode, RetryReason::ConfigCacheMiss);
                    continue;
                }
                record_timeout_if_any(ctx.mode, &error);
                return Err(error);
            }
            Ok(response) => {
                slot.mark_synced(
                    Arc::clone(target.egress_sync_key),
                    Arc::clone(target.egress_hash),
                );
                slot.mark_synced(
                    Arc::clone(target.supplier_sync_key),
                    Arc::clone(target.supplier_hash),
                );
                return Ok(invoke_response_to_upstream(response));
            }
            Err(error) => {
                record_timeout_if_any(ctx.mode, &error.to_string());
                if backoff_before_retry(&mut attempt, ctx.max_retries).await {
                    tracing::info!(
                        target: mazbri_core::trace_spans::SPAN_EVENT_TARGET,
                        retry = attempt,
                        error = %error,
                        "upstream.transport_retry"
                    );
                    record_retry(ctx.mode, RetryReason::Transport);
                    slot = state
                        .upstream_http2_pool
                        .pick_slot(BYPASS_CONTRACT_ID, bypass_addr)
                        .await
                        .map_err(|e| format!("upstream HTTP/2 request failed: {e}"))?;
                    continue;
                }
                return Err(format!("upstream HTTP/2 request failed: {error}"));
            }
        }
    }
}

fn upstream_request_id_of(meta: Option<&upstream_proto::InvokeUpstreamMeta>) -> String {
    meta.map_or_default(|m| m.upstream_request_id.clone())
}

fn upstream_wire_duration_of(meta: Option<&upstream_proto::InvokeUpstreamMeta>) -> Duration {
    let Some(m) = meta else {
        return Duration::ZERO;
    };
    if m.upstream_wire_us > 0 {
        Duration::from_micros(m.upstream_wire_us)
    } else {
        Duration::from_millis(m.upstream_wire_ms)
    }
}

fn invoke_response_to_upstream(response: upstream_proto::InvokeResponse) -> UpstreamResponse {
    let meta = response.upstream_meta.as_ref();
    let upstream_request_id = upstream_request_id_of(meta);
    let wire_duration = upstream_wire_duration_of(meta);

    if let Some(biz) = response.biz_error {
        let fallback = || {
            serde_json::json!({
                "code": biz.code,
                "message": biz.message,
            })
        };
        let body = if biz.details_json.is_empty() {
            fallback()
        } else {
            serde_json::from_str(&biz.details_json).unwrap_or_else(|_| fallback())
        };
        let bytes_len = serde_json::to_vec(&body).map_or(0, |b| b.len() as u64);
        return UpstreamResponse {
            status: 400,
            body: ProxyBody::JsonValue(body),
            error: biz.message,
            content_type: Some("application/json".into()),
            body_bytes_len: bytes_len,
            upstream_wire_duration: wire_duration,
            upstream_request_id,
        };
    }

    let status = meta
        .map(|m| u16::saturating_from(m.status))
        .filter(|s| *s > 0)
        .unwrap_or(200);
    let body_bytes_len = response.payload_json.len() as u64;
    let body = if response.payload_json.is_empty() {
        ProxyBody::JsonRawStr(Arc::from("{}"))
    } else if serde_json::from_str::<&RawValue>(&response.payload_json).is_ok() {
        ProxyBody::JsonRawStr(Arc::from(response.payload_json))
    } else {
        ProxyBody::JsonValue(serde_json::Value::String(response.payload_json))
    };

    UpstreamResponse {
        status,
        body,
        error: String::new(),
        content_type: Some("application/json".into()),
        body_bytes_len,
        upstream_wire_duration: wire_duration,
        upstream_request_id,
    }
}

async fn execute_plain_request(
    state: &ProxyState,
    contract_id: &str,
    addr: &str,
    request: upstream_proto::HttpUpstreamRequest,
    ctx: AttemptCtx<'_>,
) -> Result<UpstreamResponse, String> {
    let mut attempt = 0u32;
    let mut next_request = Some(request.clone());
    loop {
        let req_to_send = next_request.take().unwrap_or_else(|| request.clone());
        let call_started = Instant::now();
        let response = state
            .upstream_http2_pool
            .execute_request(contract_id, addr, req_to_send)
            .await;
        let wire_duration = call_started.elapsed();
        match response {
            Ok(response) => {
                if !response.success {
                    let error = if response.error.is_empty() {
                        "upstream request failed".to_string()
                    } else {
                        response.error
                    };
                    record_timeout_if_any(ctx.mode, &error);
                    return Err(error);
                }
                let data = response
                    .data
                    .ok_or_else(|| "http upstream response missing data".to_string())?;
                let body = map_proto_upstream_body(data.body.unwrap_or_default());
                let content_type = data.headers.get("content-type").cloned();
                return Ok(UpstreamResponse {
                    status: u16::saturating_from(data.status),
                    body,
                    error: String::new(),
                    content_type,
                    body_bytes_len: data.body_bytes_len,
                    upstream_wire_duration: wire_duration,
                    upstream_request_id: String::new(),
                });
            }
            Err(error) => {
                record_timeout_if_any(ctx.mode, &error.to_string());
                if backoff_before_retry(&mut attempt, ctx.max_retries).await {
                    tracing::info!(
                        target: mazbri_core::trace_spans::SPAN_EVENT_TARGET,
                        retry = attempt,
                        error = %error,
                        "upstream.transport_retry"
                    );
                    record_retry(ctx.mode, RetryReason::Transport);
                    continue;
                }
                return Err(format!("upstream HTTP/2 request failed: {error}"));
            }
        }
    }
}

pub(super) fn map_proto_upstream_body(body: upstream_proto::UpstreamResponseBody) -> ProxyBody {
    match body.kind {
        Some(upstream_proto::upstream_response_body::Kind::JsonValue(value)) => {
            if serde_json::from_str::<&RawValue>(&value).is_ok() {
                ProxyBody::JsonRawStr(Arc::from(value))
            } else {
                ProxyBody::JsonValue(serde_json::Value::String(value))
            }
        }
        Some(upstream_proto::upstream_response_body::Kind::JsonRaw(raw)) => {
            ProxyBody::JsonRaw(Arc::from(raw.into_boxed_slice()))
        }
        Some(upstream_proto::upstream_response_body::Kind::Text(text)) => ProxyBody::Text(text),
        Some(upstream_proto::upstream_response_body::Kind::Binary(binary)) => {
            ProxyBody::Binary(binary.bytes)
        }
        None => ProxyBody::Empty,
    }
}

pub(super) fn proxy_body_to_upstream_body_proto(
    body: &GatewayRequestBody,
) -> upstream_proto::UpstreamBody {
    upstream_proto::UpstreamBody {
        kind: Some(upstream_proto::upstream_body::Kind::Json(
            body.as_raw_json_str().to_string(),
        )),
    }
}

pub(super) fn sanitize_client_ip_headers(
    headers: HashMap<String, String>,
) -> HashMap<String, String> {
    headers
        .into_iter()
        .filter(|(key, _)| !is_client_ip_header(key))
        .collect()
}

pub(super) fn is_client_ip_header(header_name: &str) -> bool {
    const BLOCKED: &[&str] = &[
        "forwarded",
        "forwarded-for",
        "x-client-ip",
        "x-cluster-client-ip",
        "x-forwarded",
        "x-forwarded-for",
        "x-forwarded-client-ip",
        "x-forwarded-for-original",
        "x-original-forwarded-for",
        "x-proxy-user-ip",
        "x-real-ip",
        "x-remote-ip",
        "true-client-ip",
        "cf-connecting-ip",
        "fastly-client-ip",
    ];
    BLOCKED
        .iter()
        .any(|blocked| header_name.eq_ignore_ascii_case(blocked))
}

#[cfg(test)]
mod internal_retries_tests {
    use super::{
        Duration, ProxyBody, RetryReason, invoke_response_to_upstream, record_internal_retries,
        record_retry, upstream_proto,
    };
    use crate::gateway::retry_ledger::scope_retry_ledger;

    #[tokio::test]
    async fn bypass_internal_retries_land_in_request_ledger() {
        let ((), ledger) = scope_retry_ledger(async {
            record_internal_retries("transparent", 1, &["auth_rejected:jsonrpc_code".into()]);
            record_retry("transparent", RetryReason::ConfigCacheMiss);
        })
        .await;
        assert_eq!(
            ledger.reasons(),
            vec!["auth_rejected:jsonrpc_code", "config_cache_miss"]
        );
    }

    #[test]
    fn invoke_response_to_upstream_preserves_raw_json_and_wire_us() {
        let resp = upstream_proto::InvokeResponse {
            success: true,
            error: String::new(),
            biz_error: None,
            payload_json: r#"{"hello":"world"}"#.into(),
            upstream_meta: Some(upstream_proto::InvokeUpstreamMeta {
                status: 201,
                upstream_wire_ms: 0,
                upstream_wire_us: 750,
                upstream_request_id: "up-1".into(),
                ..Default::default()
            }),
            internal_retries: 0,
            retry_reasons: Vec::new(),
        };
        let upstream = invoke_response_to_upstream(resp);
        assert_eq!(upstream.status, 201);
        assert_eq!(upstream.upstream_wire_duration, Duration::from_micros(750));
        assert_eq!(upstream.upstream_request_id, "up-1");
        match upstream.body {
            ProxyBody::JsonRawStr(s) => assert_eq!(s.as_ref(), r#"{"hello":"world"}"#),
            other => panic!("expected JsonRawStr, got {other:?}"),
        }
    }
}
