use super::{
    API_PROXY_REQUESTS_TOTAL, BYPASS_CONTRACT_ID, CBPRO_FALLBACK_TTL_SECS, CandidateChannel,
    CircuitResult, Deserialize, Duration, FallbackSnapshot, GatewayRequest, InteractionEvent,
    MazbriClaims, ObservationRequestParts, ProxyError, ProxyState, StoreRequest, SwitchCondition,
    TraceContext, UpstreamResponse, build_fallback_cache_key, build_observation_request,
    classify_proxy_result, enqueue_cbpro_observation, execute_http_upstream_request,
    get_or_create_breaker, info, persist_interaction_event, proxy_body_to_json_value, warn,
};

pub(super) async fn persist_event_best_effort(state: &ProxyState, event: &InteractionEvent) {
    if let Err(error) = persist_interaction_event(&state.db, event).await {
        warn!(
            event_id = % event.id, request_id = % event.request_id, error = % error,
            "Failed to persist gateway interaction event"
        );
    }
}
pub(super) fn spawn_persist_event_best_effort(state: &ProxyState, event: &InteractionEvent) {
    let state = state.clone();
    let event = event.clone();
    tokio::spawn(async move {
        persist_event_best_effort(&state, &event).await;
    });
}
pub(super) fn should_switch(error: &ProxyError, condition: &SwitchCondition) -> bool {
    match error {
        ProxyError::GatewayTimeout(_) => condition.switch_on_timeout,
        ProxyError::BadGateway(_) => condition.switch_on_bad_gateway,
        ProxyError::InternalError(_) => condition.switch_on_internal_error,
        _ => false,
    }
}
pub(super) fn should_switch_status(status: u16, condition: &SwitchCondition) -> bool {
    condition.switch_on_status_codes.contains(&status)
}
pub(super) async fn execute_channel_attempt(
    state: &ProxyState,
    req: &GatewayRequest,
    channel: &CandidateChannel,
    claims: &MazbriClaims,
    request_id: &str,
    trace_context: &TraceContext,
) -> Result<UpstreamResponse, ProxyError> {
    let addr = channel.supplier_upstream_url.as_str();
    let (mode, _contract) = resolve_mode_and_contract_hint(channel);
    info!(
        "[{}] Attempting channel {} at {} for tenant [{}] mode={}",
        request_id, channel.channel_id, addr, claims.tenant_code, mode
    );
    let breaker = get_or_create_breaker(state, &req.product_id).await;
    let product_id = req.product_id.clone();
    let method = req.method.clone();
    let timeout_duration = Duration::from_millis(channel.switch_condition.timeout_ms);
    let timeout_result = tokio::time::timeout(
        timeout_duration,
        breaker.execute(|| async {
            execute_http_upstream_request(
                state,
                req,
                channel,
                claims,
                channel.switch_condition.max_retries,
                trace_context,
            )
            .await
        }),
    )
    .await;
    if let Ok(result) = timeout_result {
        match result {
            CircuitResult::Success(response) => {
                let status = response.status.to_string();
                let result_label = classify_proxy_result(response.status, None);
                record_proxy_metric(&product_id, &method, &status, &mode, result_label);
                Ok(response)
            }
            CircuitResult::Rejected {
                reason,
                retry_after,
            } => {
                warn!(
                    "[{}] Circuit breaker rejected request for product [{}]: {} (retry_after: {:?})",
                    request_id, product_id, reason, retry_after
                );
                record_proxy_metric(&product_id, &method, "503", &mode, "circuit_open");
                Err(ProxyError::BadGateway(format!("服务暂时不可用: {reason}")))
            }
            CircuitResult::Failure { error } => {
                warn!(
                    "[{}] upstream HTTP/2 request failed for product [{}]: {}",
                    request_id, product_id, error
                );
                let result_label = if error.contains("contract") || error.contains("contract_id") {
                    "config"
                } else {
                    "upstream_error"
                };
                record_proxy_metric(&product_id, &method, "502", &mode, result_label);
                Err(ProxyError::BadGateway(error))
            }
        }
    } else {
        warn!(
            "[{}] upstream HTTP/2 request timed out for product [{}]",
            request_id, product_id
        );
        record_proxy_metric(&product_id, &method, "504", &mode, "timeout");
        Err(ProxyError::GatewayTimeout(format!(
            "channel wait exceeded switch_condition.timeout_ms for product={product_id}"
        )))
    }
}

fn record_proxy_metric(product_id: &str, method: &str, status: &str, mode: &str, result: &str) {
    API_PROXY_REQUESTS_TOTAL
        .with_label_values(&[product_id, method, status, mode, result])
        .inc();
}

/// 从 `channel.supplier_config` 解析 mode / contract 提示（失败时 unknown，正式路径在 execute 内再校验）。
pub(super) fn resolve_mode_and_contract_hint(channel: &CandidateChannel) -> (String, String) {
    match mazbri_core::SupplierConfig::deserialize(&channel.supplier_config) {
        Ok(cfg) if cfg.is_transparent() => {
            let adapter = mazbri_core::parse_channel_config_egress(&channel.channel_config)
                .map_or_else(|_| "transparent".to_string(), |s| s.adapter);
            (adapter, BYPASS_CONTRACT_ID.to_string())
        }
        Ok(cfg) if cfg.is_microservice() => {
            let contract = cfg
                .contract_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("unknown")
                .to_string();
            ("microservice".to_string(), contract)
        }
        _ => ("unknown".to_string(), "unknown".to_string()),
    }
}
pub(super) async fn store_fallback_snapshot(
    state: &ProxyState,
    req: &GatewayRequest,
    claims: &MazbriClaims,
    request_id: &str,
    response: &UpstreamResponse,
) {
    if !(200..300).contains(&response.status) {
        return;
    }
    let Some(cbpro_client) = state.cbpro_client.as_ref() else {
        return;
    };
    let cache_key = build_fallback_cache_key(req);
    let body_json = proxy_body_to_json_value(&response.body);
    let request = StoreRequest {
        product_id: req.product_id.clone(),
        tenant_id: claims.tenant_id.clone(),
        cache_key: cache_key.clone(),
        data: serde_json::json!(FallbackSnapshot {
            status: response.status,
            data: body_json.clone(),
        }),
        ttl_secs: CBPRO_FALLBACK_TTL_SECS,
        tags: vec![req.method.clone(), req.path.clone()],
    };
    // Probe connectivity first; if the channel is stale this forces invalidate+reconnect
    // before the larger store_response payload is sent.
    if let Err(error) = cbpro_client.health_check().await {
        warn!(
            product_id = %req.product_id,
            tenant_id = %claims.tenant_id,
            error = %error,
            "CBPro health_check failed before snapshot store"
        );
    }
    let store_response = cbpro_client.store_response(request).await;
    if !store_response.success {
        // Fail-open for the tenant request, but surface store failures at warn so
        // ops can see snapshot loss (dead tarpc channel, cbpro down, etc.).
        warn!(
            cache_key = %cache_key,
            product_id = %req.product_id,
            tenant_id = %claims.tenant_id,
            error = ?store_response.error,
            "CBPro snapshot store failed (request still succeeded upstream)"
        );
    }
    let observation_payload = serde_json::json!({
        "status": response.status,
        "data": body_json,
    });
    enqueue_cbpro_observation(
        state,
        build_observation_request(ObservationRequestParts {
            event_type: "upstream_success",
            request_id,
            tenant_id: &claims.tenant_id,
            product_id: &req.product_id,
            method: &req.method,
            path: &req.path,
            cache_key: Some(cache_key),
            status: Some(response.status),
            service_total_ms: None,
            response: Some(&observation_payload),
        }),
    );
}
