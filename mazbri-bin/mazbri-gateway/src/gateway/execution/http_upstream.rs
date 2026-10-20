use mazbri_core::num::SaturatingFrom;
use super::*;

pub(super) async fn execute_http_upstream_request(
    state: &ProxyState,
    req: &GatewayRequest,
    channel: &CandidateChannel,
    claims: &MazbriClaims,
    max_retries: u32,
    trace_context: &TraceContext,
) -> Result<UpstreamResponse, String> {
    let addr = channel.supplier_upstream_url.as_str();

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

    // 契约身份与业务 channel 分离：transparent 固定 BYPASS；microservice 读 config.contract_id
    let contract_id = if is_transparent {
        BYPASS_CONTRACT_ID.to_string()
    } else {
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
        id.to_string()
    };

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

    let mut final_addr = addr.to_string();
    let mut invoke_req: Option<upstream_proto::InvokeRequest> = None;

    if is_transparent {
        let spec = mazbri_core::parse_channel_config_egress(&channel.channel_config)
            .map_err(|e| e.to_string())?;
        let adapter_name = spec.adapter.clone();
        headers.insert("x-mazbri-upstream-mode".to_string(), adapter_name.clone());

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
        let egress_json = serde_json::to_string(&spec).unwrap_or_else(|_| "{}".into());
        let egress_hash =
            crate::supplier_config_cache::SupplierConfigSendCache::hash_config_json(&egress_json);
        let config_json = serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string());
        let config_hash =
            crate::supplier_config_cache::SupplierConfigSendCache::hash_config_json(&config_json);
        let spec_cache_key = format!("egress:{}", channel.channel_id);
        let cfg_cache_key = format!("supplier:{}", channel.supplier_code);

        let send_spec_full = state
            .supplier_config_send_cache
            .should_send_full(&spec_cache_key, &egress_hash);
        let send_cfg_full = state
            .supplier_config_send_cache
            .should_send_full(&cfg_cache_key, &config_hash);
        if send_spec_full {
            state
                .supplier_config_send_cache
                .mark_full_sent(&spec_cache_key, &egress_hash);
        }
        if send_cfg_full {
            state
                .supplier_config_send_cache
                .mark_full_sent(&cfg_cache_key, &config_hash);
        }

        let timeout_ms = spec
            .timeout_ms
            .or_else(|| config.timeouts.as_ref().and_then(|t| t.total_ms))
            .unwrap_or(timeout_secs.saturating_mul(1000));

        let bypass_endpoint = state.services_config.bypass.as_ref().ok_or_else(|| {
            "Bypass service endpoint is not configured in ServicesConfig (transparent upstream requires bypass_url)"
                .to_string()
        })?;
        let bypass_host_port = bypass_endpoint.url();
        final_addr = format!("https://{bypass_host_port}");

        invoke_req = Some(upstream_proto::InvokeRequest {
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
                egress_json
            } else {
                String::new()
            },
            supplier_id: channel.supplier_id.clone(),
            supplier_code: channel.supplier_code.clone(),
            supplier_config_hash: config_hash,
            supplier_config_json: if send_cfg_full {
                config_json
            } else {
                String::new()
            },
            fallback_base_url: addr.to_string(),
            path_prefix: channel.path_prefix.clone(),
        });
        let _ = adapter_name;
    }

    if let Some(mut request) = invoke_req {
        let mut attempt = 0u32;
        let mut forced_full_config = false;
        loop {
            let response = state
                .upstream_http2_pool
                .execute_invoke(&contract_id, &final_addr, None, request.clone())
                .await;
            match response {
                Ok(response) => {
                    if !response.success {
                        let error = if response.error.is_empty() {
                            "upstream request failed".to_string()
                        } else {
                            response.error
                        };
                        if !forced_full_config && error.contains("CONFIG_CACHE_MISS") {
                            let spec =
                                mazbri_core::parse_channel_config_egress(&channel.channel_config)
                                    .map_err(|e| e.to_string())?;
                            request.egress_spec_json =
                                serde_json::to_string(&spec).unwrap_or_else(|_| "{}".into());
                            request.supplier_config_json =
                                serde_json::to_string(&channel.supplier_config)
                                    .unwrap_or_else(|_| "{}".into());
                            state
                                .supplier_config_send_cache
                                .invalidate(&format!("egress:{}", channel.channel_id));
                            state
                                .supplier_config_send_cache
                                .invalidate(&format!("supplier:{}", channel.supplier_code));
                            forced_full_config = true;
                            continue;
                        }
                        return Err(error);
                    }
                    if let Some(ref biz) = response.biz_error {
                        let body = if biz.details_json.is_empty() {
                            serde_json::json!({
                                "code": biz.code,
                                "message": biz.message,
                            })
                        } else {
                            serde_json::from_str(&biz.details_json).unwrap_or(serde_json::json!({
                                "code": biz.code,
                                "message": biz.message,
                            }))
                        };
                        let bytes_len = serde_json::to_vec(&body)
                            .map_or(0, |b| b.len() as u64);
                        return Ok(UpstreamResponse {
                            status: 400,
                            body: ProxyBody::JsonValue(body),
                            error: biz.message.clone(),
                            content_type: Some("application/json".into()),
                            body_bytes_len: bytes_len,
                        });
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
                    let bytes_len = response.payload_json.len() as u64;
                    return Ok(UpstreamResponse {
                        status,
                        body: ProxyBody::JsonValue(payload),
                        error: String::new(),
                        content_type: Some("application/json".into()),
                        body_bytes_len: bytes_len,
                    });
                }
                Err(error) => {
                    if attempt < max_retries {
                        use rand::RngExt;
                        let cap = 100u64 * 2u64.pow(attempt);
                        let delay_ms = rand::rng().random_range(0..=cap);
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(format!("upstream HTTP/2 request failed: {error}"));
                }
            }
        }
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
    let mut attempt = 0u32;
    loop {
        let response = state
            .upstream_http2_pool
            .execute_request(&contract_id, &final_addr, None, request.clone())
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
                if attempt < max_retries {
                    use rand::RngExt;
                    let cap = 100u64 * 2u64.pow(attempt);
                    let delay_ms = rand::rng().random_range(0..=cap);
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    attempt += 1;
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
