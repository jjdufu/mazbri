use super::*;
use mazbri_core::num::SaturatingFrom;
use std::sync::Arc;

pub(super) async fn execute_http_upstream_request(
    state: &ProxyState,
    req: &GatewayRequest,
    channel: &CandidateChannel,
    claims: &MazbriClaims,
    max_retries: u32,
    trace_context: &TraceContext,
) -> Result<UpstreamResponse, String> {
    // 唯一真相源：mazbri_core::SupplierConfig；分发只认 upstream_type（禁止 URL 启发式 / Default 吞错）
    // Deserialize from borrowed Value — no clone of the full JSON tree.
    let config = mazbri_core::SupplierConfig::deserialize(&channel.supplier_config)
        .map_err(|e| format!("invalid supplier_config JSON: {e}"))?;
    if !config.is_transparent() && !config.is_microservice() {
        return Err(format!(
            "invalid upstream_type: '{}' (expect transparent|microservice)",
            config.upstream_type
        ));
    }
    let is_transparent = config.is_transparent();
    let contract_id = resolve_contract_id(&config)?;

    // 超时：优先 supplier.timeouts.total_ms，否则 0 表示 bypass/上游使用默认
    let timeout_secs = config
        .timeouts
        .as_ref()
        .and_then(|t| t.total_ms)
        .map_or(0, |ms| ms.div_ceil(1000).max(1));

    let mut headers = sanitize_client_ip_headers(req.headers.clone().unwrap_or_default());

    // Write correlation headers in-place (no intermediate HashMap).
    trace_context.write_propagate_headers(&mut headers);
    headers.insert(
        "x-mazbri-upstream-mode".to_string(),
        if is_transparent {
            "transparent".into()
        } else {
            "microservice".into()
        },
    );
    headers.insert("x-mazbri-contract-id".to_string(), contract_id.clone());

    if is_transparent {
        let target = TransparentTarget {
            state,
            req,
            channel,
            claims,
            trace_context,
            config: &config,
            timeout_secs,
        };
        let (bypass_addr, invoke_req) = build_invoke_request(&target, &mut headers)?;
        return invoke_bypass(
            state,
            channel,
            &contract_id,
            &bypass_addr,
            invoke_req,
            max_retries,
        )
        .await;
    }

    let request = upstream_proto::HttpUpstreamRequest {
        product_id: req.product_id.clone(),
        tenant_id: claims.tenant_id.clone(),
        path: req.path.clone(),
        method: req.method.clone(),
        query: req.query.clone().unwrap_or_default(),
        headers,
        body: req.body.as_ref().map(proxy_body_to_upstream_body_proto),
        timeout_secs,
    };
    execute_plain_request(
        state,
        &contract_id,
        channel.supplier_upstream_url.as_str(),
        request,
        max_retries,
    )
    .await
}

/// 契约身份与业务 channel 分离：transparent 固定 BYPASS；microservice 读 `config.contract_id`。
fn resolve_contract_id(config: &mazbri_core::SupplierConfig) -> Result<String, String> {
    if config.is_transparent() {
        return Ok(BYPASS_CONTRACT_ID.to_string());
    }
    let id = config
        .contract_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "microservice supplier missing config.contract_id".to_string())?;
    // 预检未知契约，便于 switch 下一通道并打 contract_errors 指标
    if let Err(e) = grpc_path_for_contract(id) {
        GATEWAY_UPSTREAM_CONTRACT_ERRORS_TOTAL
            .with_label_values(&[id, "unknown_contract"])
            .inc();
        return Err(format!("upstream contract error: {e}"));
    }
    Ok(id.to_string())
}

struct TransparentTarget<'a> {
    state: &'a ProxyState,
    req: &'a GatewayRequest,
    channel: &'a CandidateChannel,
    claims: &'a MazbriClaims,
    trace_context: &'a TraceContext,
    config: &'a mazbri_core::SupplierConfig,
    timeout_secs: u64,
}

/// 增量下发：内容未变时只发 hash，bypass 缓存未命中再回退全量。
fn should_send_full_and_mark(state: &ProxyState, cache_key: &str, hash: &str) -> bool {
    let send_full = state
        .supplier_config_send_cache
        .should_send_full(cache_key, hash);
    if send_full {
        state
            .supplier_config_send_cache
            .mark_full_sent(cache_key, hash);
    }
    send_full
}

/// Parsed egress spec + its serialized JSON and SHA256, cached per channel and reused
/// while the source `channel_config` is unchanged (content equality = config version).
struct EgressEntry {
    source: serde_json::Value,
    spec: mazbri_core::ChannelEgressSpec,
    json: String,
    hash: String,
}

/// Serialized supplier config + SHA256, cached per supplier while the config is unchanged.
struct SupplierEntry {
    source: mazbri_core::SupplierConfig,
    json: String,
    hash: String,
}

static EGRESS_CACHE: std::sync::LazyLock<dashmap::DashMap<String, Arc<EgressEntry>>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);
static SUPPLIER_CACHE: std::sync::LazyLock<dashmap::DashMap<String, Arc<SupplierEntry>>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

fn sha_hex(json: &str) -> String {
    crate::supplier_config_cache::SupplierConfigSendCache::hash_config_json(json)
}

fn cached_egress(channel_id: &str, source: &serde_json::Value) -> Result<Arc<EgressEntry>, String> {
    if let Some(e) = EGRESS_CACHE.get(channel_id)
        && e.source == *source
    {
        return Ok(Arc::clone(e.value()));
    }
    let spec = mazbri_core::parse_channel_config_egress(source).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(&spec).unwrap_or_else(|_| "{}".into());
    let entry = Arc::new(EgressEntry {
        source: source.clone(),
        spec,
        hash: sha_hex(&json),
        json,
    });
    EGRESS_CACHE.insert(channel_id.to_string(), Arc::clone(&entry));
    Ok(entry)
}

fn cached_supplier(
    supplier_code: &str,
    source: &mazbri_core::SupplierConfig,
) -> Arc<SupplierEntry> {
    if let Some(e) = SUPPLIER_CACHE.get(supplier_code)
        && e.source == *source
    {
        return Arc::clone(e.value());
    }
    let json = serde_json::to_string(source).unwrap_or_else(|_| "{}".to_string());
    let entry = Arc::new(SupplierEntry {
        source: source.clone(),
        hash: sha_hex(&json),
        json,
    });
    SUPPLIER_CACHE.insert(supplier_code.to_string(), Arc::clone(&entry));
    entry
}

/// transparent 免代码路径：组装发往 bypass 的 `InvokeRequest`，返回 `(bypass 地址, 请求)`。
fn build_invoke_request(
    target: &TransparentTarget<'_>,
    headers: &mut HashMap<String, String>,
) -> Result<(String, upstream_proto::InvokeRequest), String> {
    let TransparentTarget {
        state,
        req,
        channel,
        claims,
        trace_context,
        config,
        timeout_secs,
    } = *target;
    let egress = cached_egress(&channel.channel_id, &channel.channel_config)?;
    let spec = &egress.spec;
    headers.insert("x-mazbri-upstream-mode".to_string(), spec.adapter.clone());

    let payload = match req.body.as_ref() {
        Some(GatewayRequestBody::Json(v)) if v.is_object() => v.clone(),
        None | Some(GatewayRequestBody::Empty) => serde_json::json!({}),
        _ => {
            return Err(
                "transparent 免代码路径只接受 JSON 对象 body（XML/form/protobuf 由 bypass 组包）"
                    .into(),
            );
        }
    };
    let payload_json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
    let supplier = cached_supplier(&channel.supplier_code, config);
    let egress_hash = egress.hash.clone();
    let config_hash = supplier.hash.clone();
    let send_spec_full = should_send_full_and_mark(
        state,
        &format!("egress:{}", channel.channel_id),
        &egress_hash,
    );
    let send_cfg_full = should_send_full_and_mark(
        state,
        &format!("supplier:{}", channel.supplier_code),
        &config_hash,
    );

    let timeout_ms = spec
        .timeout_ms
        .or_else(|| config.timeouts.as_ref().and_then(|t| t.total_ms))
        .unwrap_or(timeout_secs.saturating_mul(1000));

    let bypass_endpoint = state.services_config.bypass.as_ref().ok_or_else(|| {
        "Bypass service endpoint is not configured in ServicesConfig (transparent upstream requires bypass_url)"
            .to_string()
    })?;
    let bypass_addr = format!("https://{}", bypass_endpoint.url());

    let invoke_req = upstream_proto::InvokeRequest {
        product_id: req.product_id.clone(),
        tenant_id: claims.tenant_id.clone(),
        channel_id: channel.channel_id.clone(),
        request_id: headers
            .get("x-request-id")
            .cloned()
            .unwrap_or_else(|| trace_context.request_id.to_string()),
        trace: Some(upstream_proto::TraceContext {
            request_id: trace_context.request_id.to_string(),
            trace_id: trace_context.trace_id.to_string(),
            span_id: trace_context.span_id.to_string(),
            parent_span_id: trace_context.parent_span_id.to_string(),
        }),
        timeout_ms,
        payload_json,
        egress_spec_hash: egress_hash,
        egress_spec_json: if send_spec_full {
            egress.json.clone()
        } else {
            String::new()
        },
        supplier_id: channel.supplier_id.clone(),
        supplier_code: channel.supplier_code.clone(),
        supplier_config_hash: config_hash,
        supplier_config_json: if send_cfg_full {
            supplier.json.clone()
        } else {
            String::new()
        },
    };
    Ok((bypass_addr, invoke_req))
}

/// 指数退避 + 抖动；返回 `false` 表示重试次数已用尽。
async fn backoff_before_retry(attempt: &mut u32, max_retries: u32) -> bool {
    use rand::RngExt;
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
    channel: &CandidateChannel,
    contract_id: &str,
    bypass_addr: &str,
    mut request: upstream_proto::InvokeRequest,
    max_retries: u32,
) -> Result<UpstreamResponse, String> {
    let mut attempt = 0u32;
    let mut forced_full_config = false;
    loop {
        let response = state
            .upstream_http2_pool
            .execute_invoke(contract_id, bypass_addr, request.clone())
            .await;
        match response {
            Ok(response) if !response.success => {
                let error = if response.error.is_empty() {
                    "upstream request failed".to_string()
                } else {
                    response.error
                };
                if !forced_full_config && error.contains("CONFIG_CACHE_MISS") {
                    force_full_config(state, channel, &mut request)?;
                    forced_full_config = true;
                    continue;
                }
                return Err(error);
            }
            Ok(response) => return Ok(invoke_response_to_upstream(&response)),
            Err(error) => {
                if backoff_before_retry(&mut attempt, max_retries).await {
                    continue;
                }
                return Err(format!("upstream HTTP/2 request failed: {error}"));
            }
        }
    }
}

/// bypass 报 `CONFIG_CACHE_MISS`：作废发送缓存并改为携带全量配置。
fn force_full_config(
    state: &ProxyState,
    channel: &CandidateChannel,
    request: &mut upstream_proto::InvokeRequest,
) -> Result<(), String> {
    let spec = mazbri_core::parse_channel_config_egress(&channel.channel_config)
        .map_err(|e| e.to_string())?;
    request.egress_spec_json = serde_json::to_string(&spec).unwrap_or_else(|_| "{}".into());
    request.supplier_config_json =
        serde_json::to_string(&channel.supplier_config).unwrap_or_else(|_| "{}".into());
    state
        .supplier_config_send_cache
        .invalidate(&format!("egress:{}", channel.channel_id));
    state
        .supplier_config_send_cache
        .invalidate(&format!("supplier:{}", channel.supplier_code));
    Ok(())
}

fn invoke_response_to_upstream(response: &upstream_proto::InvokeResponse) -> UpstreamResponse {
    if let Some(ref biz) = response.biz_error {
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
            error: biz.message.clone(),
            content_type: Some("application/json".into()),
            body_bytes_len: bytes_len,
        };
    }
    let payload = if response.payload_json.is_empty() {
        serde_json::Value::Object(serde_json::Map::default())
    } else {
        serde_json::from_str(&response.payload_json)
            .unwrap_or(serde_json::Value::String(response.payload_json.clone()))
    };
    let status = response
        .upstream_meta
        .as_ref()
        .map(|m| u16::saturating_from(m.status))
        .filter(|s| *s > 0)
        .unwrap_or(200);
    UpstreamResponse {
        status,
        body: ProxyBody::JsonValue(payload),
        error: String::new(),
        content_type: Some("application/json".into()),
        body_bytes_len: response.payload_json.len() as u64,
    }
}

async fn execute_plain_request(
    state: &ProxyState,
    contract_id: &str,
    addr: &str,
    request: upstream_proto::HttpUpstreamRequest,
    max_retries: u32,
) -> Result<UpstreamResponse, String> {
    let mut attempt = 0u32;
    loop {
        let response = state
            .upstream_http2_pool
            .execute_request(contract_id, addr, request.clone())
            .await;
        match response {
            Ok(response) => {
                if !response.success {
                    let error = if response.error.is_empty() {
                        "upstream request failed".to_string()
                    } else {
                        response.error
                    };
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
                });
            }
            Err(error) => {
                if backoff_before_retry(&mut attempt, max_retries).await {
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
            ProxyBody::JsonValue(
                serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value)),
            )
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
pub(super) fn proxy_body_to_json_value(body: &ProxyBody) -> serde_json::Value {
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
pub(super) fn proxy_body_to_upstream_body_proto(
    body: &GatewayRequestBody,
) -> upstream_proto::UpstreamBody {
    match body {
        GatewayRequestBody::Json(value) => upstream_proto::UpstreamBody {
            kind: Some(upstream_proto::upstream_body::Kind::Json(value.to_string())),
        },
        GatewayRequestBody::Raw(value) => upstream_proto::UpstreamBody {
            kind: Some(upstream_proto::upstream_body::Kind::Raw(value.clone())),
        },
        GatewayRequestBody::Empty => upstream_proto::UpstreamBody { kind: None },
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
    matches!(
        header_name.to_ascii_lowercase().as_str(),
        "forwarded"
            | "forwarded-for"
            | "x-client-ip"
            | "x-cluster-client-ip"
            | "x-forwarded"
            | "x-forwarded-for"
            | "x-forwarded-client-ip"
            | "x-forwarded-for-original"
            | "x-original-forwarded-for"
            | "x-proxy-user-ip"
            | "x-real-ip"
            | "x-remote-ip"
            | "true-client-ip"
            | "cf-connecting-ip"
            | "fastly-client-ip"
    )
}
