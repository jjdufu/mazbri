use super::{
    API_PROXY_REQUESTS_TOTAL, AttemptCtx, BYPASS_CONTRACT_ID, CBPRO_FALLBACK_TTL_SECS,
    CandidateChannel, CircuitResult, Deserialize, Duration, FallbackSnapshot, GatewayRequest,
    InteractionEvent, MazbriClaims, ObservationRequestParts, ProxyError, ProxyState, StoreRequest,
    SwitchCondition, TimeoutPhase, TraceContext, UpstreamResponse, build_fallback_cache_key,
    build_observation_request, classify_proxy_result, enqueue_cbpro_observation,
    execute_http_upstream_request, get_or_create_breaker, info, persist_interaction_event,
    proxy_body_to_json_value, record_upstream_timeout, warn,
};
use mazbri_upstream_contracts::UPSTREAM_AUTH_REJECTED_PREFIX;
use tracing::Instrument;

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
        ProxyError::BadGateway(_) | ProxyError::UpstreamAuthRejected(_) => {
            condition.switch_on_bad_gateway
        }
        ProxyError::InternalError(_) => condition.switch_on_internal_error,
        _ => false,
    }
}
pub(super) fn should_switch_status(status: u16, condition: &SwitchCondition) -> bool {
    condition.switch_on_status_codes.contains(&status)
}
/// 一次候选通道尝试：通道与其在本请求内的尝试序号（从 1 起）。
#[derive(Clone, Copy)]
pub(super) struct ChannelAttempt<'a> {
    pub(super) channel: &'a CandidateChannel,
    pub(super) order: usize,
}

impl<'a> ChannelAttempt<'a> {
    pub(super) fn new(channel: &'a CandidateChannel, order: usize) -> Self {
        Self { channel, order }
    }
}

/// 每个候选通道一次尝试一个 `upstream.attempt` span；超时/失败标 error，
/// 随后切换到的通道是下一个 attempt（`attempt_order` 从 1 起）。
pub(super) async fn execute_channel_attempt(
    state: &ProxyState,
    req: &GatewayRequest,
    attempt: ChannelAttempt<'_>,
    claims: &MazbriClaims,
    request_id: &str,
    trace_context: &TraceContext,
) -> Result<UpstreamResponse, ProxyError> {
    let channel = attempt.channel;
    let (mode, _contract) = resolve_mode_and_contract_hint(channel);
    let span = upstream_attempt_span(attempt.order, channel, &mode);
    let result = run_channel_attempt(state, req, channel, claims, request_id, trace_context, mode)
        .instrument(span.clone())
        .await;
    record_attempt_span(&span, &result);
    result
}

/// 一次候选通道尝试的 `upstream.attempt` span（结果由 [`record_attempt_span`] 补写）。
fn upstream_attempt_span(
    attempt_order: usize,
    channel: &CandidateChannel,
    mode: &str,
) -> tracing::Span {
    tracing::info_span!(
        "upstream.attempt",
        otel.kind = "client",
        attempt_order,
        supplier_code = %channel.supplier_code,
        channel_code = %channel.channel_code,
        mode = %mode,
        http.status_code = tracing::field::Empty,
        otel.status_code = tracing::field::Empty,
        error_code = tracing::field::Empty,
        error_message = tracing::field::Empty,
    )
}

/// 把一次通道尝试的结果写到 `upstream.attempt` span：HTTP 状态码，失败时 error 状态。
fn record_attempt_span(span: &tracing::Span, result: &Result<UpstreamResponse, ProxyError>) {
    match result {
        Ok(response) => {
            span.record("http.status_code", response.status);
            if response.status >= 500 {
                span.record("otel.status_code", "ERROR");
            }
        }
        Err(error) => {
            span.record("error_code", error.error_code());
            span.record("otel.status_code", "ERROR");
            span.record("error_message", error.to_string());
        }
    }
}

async fn run_channel_attempt(
    state: &ProxyState,
    req: &GatewayRequest,
    channel: &CandidateChannel,
    claims: &MazbriClaims,
    request_id: &str,
    trace_context: &TraceContext,
    mode: String,
) -> Result<UpstreamResponse, ProxyError> {
    let addr = channel.supplier_upstream_url.as_str();
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
                AttemptCtx {
                    max_retries: channel.switch_condition.max_retries,
                    mode: &mode,
                },
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
                tracing::info!(
                    target: mazbri_core::trace_spans::SPAN_EVENT_TARGET,
                    reason = %reason,
                    "circuit_breaker.rejected"
                );
                record_proxy_metric(&product_id, &method, "503", &mode, "circuit_open");
                Err(ProxyError::BadGateway(format!("服务暂时不可用: {reason}")))
            }
            CircuitResult::Failure { error } => {
                warn!(
                    "[{}] upstream HTTP/2 request failed for product [{}]: {}",
                    request_id, product_id, error
                );
                if error.starts_with(UPSTREAM_AUTH_REJECTED_PREFIX) {
                    record_proxy_metric(&product_id, &method, "502", &mode, "upstream_auth");
                    return Err(ProxyError::UpstreamAuthRejected(error));
                }
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
        tracing::info!(
            target: mazbri_core::trace_spans::SPAN_EVENT_TARGET,
            timeout_ms = channel.switch_condition.timeout_ms,
            "upstream.timeout"
        );
        record_proxy_metric(&product_id, &method, "504", &mode, "timeout");
        record_upstream_timeout(&mode, TimeoutPhase::Read);
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

#[cfg(test)]
mod upstream_auth_rejected_tests {
    use super::*;
    use crate::gateway::metrics::classify_error_class;
    use crate::gateway::post_response::billing_code_for_status;
    use crate::gateway::response::UPSTREAM_AUTH_REJECTED_CODE;
    use axum::response::IntoResponse;

    fn rejected() -> ProxyError {
        ProxyError::UpstreamAuthRejected(format!(
            "{UPSTREAM_AUTH_REJECTED_PREFIX}UPSTREAM_AUTH_REJECTED: upstream rejected credentials"
        ))
    }

    #[test]
    fn auth_rejected_is_unbilled_bad_gateway() {
        let err = rejected();
        assert_eq!(err.error_code(), UPSTREAM_AUTH_REJECTED_CODE);
        assert!(
            err.should_sanitize(),
            "internal detail never reaches tenants"
        );
        let status = err.into_response().status();
        assert_eq!(status.as_u16(), 502);
        let billing = billing_code_for_status(status.as_u16());
        assert_eq!(billing, mazbri_rpc::BillingCode::UPSTREAM_ERROR);
        assert!(!billing.default_billable());
        assert_eq!(
            classify_error_class(502, false, UPSTREAM_AUTH_REJECTED_CODE),
            "upstream_auth"
        );
        assert_eq!(classify_error_class(502, false, "E5020"), "upstream_error");
    }

    #[test]
    fn auth_rejected_switches_like_bad_gateway() {
        let on = SwitchCondition {
            switch_on_bad_gateway: true,
            ..SwitchCondition::default()
        };
        let off = SwitchCondition::default();
        assert!(should_switch(&rejected(), &on));
        assert!(!should_switch(&rejected(), &off));
    }
}
