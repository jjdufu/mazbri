use super::{
    API_PROXY_REQUESTS_TOTAL, AttemptCtx, CBPRO_FALLBACK_TTL_SECS, CircuitResult, Duration,
    FallbackSnapshot, GatewayCandidateChannel, GatewayRequest, MazbriClaims,
    ObservationRequestParts, ProxyError, ProxyState, StoreRequest, SwitchCondition, TimeoutPhase,
    TraceContext, UpstreamResponse, build_fallback_cache_key, build_observation_request,
    classify_proxy_result, debug, enqueue_cbpro_observation, execute_http_upstream_request,
    get_or_create_breaker, is_retry_safe_request, proxy_body_to_json_value,
    record_upstream_timeout, warn,
};
use mazbri_upstream_contracts::UPSTREAM_AUTH_REJECTED_PREFIX;
use tracing::Instrument;

pub(super) fn should_switch(error: &ProxyError, condition: &SwitchCondition) -> bool {
    match error {
        ProxyError::GatewayTimeout(_) => condition.switch_on_timeout,
        ProxyError::BadGateway(_)
        | ProxyError::CircuitOpen(_)
        | ProxyError::UpstreamAuthRejected(_) => condition.switch_on_bad_gateway,
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
    pub(super) channel: &'a GatewayCandidateChannel,
    pub(super) order: usize,
}

impl<'a> ChannelAttempt<'a> {
    pub(super) fn new(channel: &'a GatewayCandidateChannel, order: usize) -> Self {
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
    let (mode, _contract) = channel.prepared.mode_and_contract_hint();
    let span = upstream_attempt_span(attempt.order, channel, mode);
    let result = run_channel_attempt(state, req, attempt, claims, request_id, trace_context, mode)
        .instrument(span.clone())
        .await;
    record_attempt_span(&span, &result);
    result
}

/// 一次候选通道尝试的 `upstream.attempt` span（结果由 [`record_attempt_span`] 补写）。
fn upstream_attempt_span(
    attempt_order: usize,
    channel: &GatewayCandidateChannel,
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
    attempt: ChannelAttempt<'_>,
    claims: &MazbriClaims,
    request_id: &str,
    trace_context: &TraceContext,
    mode: &str,
) -> Result<UpstreamResponse, ProxyError> {
    let channel = attempt.channel;
    let addr = channel.supplier_upstream_url.as_str();
    debug!(
        "[{}] Attempting channel {} at {} for tenant [{}] mode={}",
        request_id, channel.channel_id, addr, claims.tenant_code, mode
    );
    let breaker = get_or_create_breaker(state, &req.product_id, channel).await;
    let product_id = req.product_id.as_str();
    let method = req.method.as_str();
    let retry_safe = is_retry_safe_request(req);
    let timeout_duration = Duration::from_millis(channel.switch_condition.timeout_ms);
    let timeout_result = tokio::time::timeout(
        timeout_duration,
        breaker.execute_with_classifier(
            || async {
                execute_http_upstream_request(
                    state,
                    req,
                    channel,
                    claims,
                    AttemptCtx {
                        max_retries: channel.switch_condition.max_retries,
                        mode,
                        attempt_order: u32::try_from(attempt.order).unwrap_or(u32::MAX),
                        retry_safe,
                    },
                    trace_context,
                )
                .await
            },
            |resp| {
                (resp.status >= 500
                    && (should_switch_status(resp.status, &channel.switch_condition)
                        || (channel.switch_condition.switch_on_bad_gateway
                            && matches!(resp.status, 502..=504))
                        || (channel.switch_condition.switch_on_internal_error
                            && resp.status == 500)))
                    .then(|| format!("HTTP status {}", resp.status))
            },
        ),
    )
    .await;
    if let Ok(result) = timeout_result {
        handle_channel_circuit_result(result, channel, product_id, method, mode, request_id)
    } else {
        breaker.record_failure("upstream timeout");
        warn!(
            "[{}] upstream HTTP/2 request timed out for channel [{}] product [{}]",
            request_id, channel.channel_code, product_id
        );
        tracing::info!(
            target: mazbri_core::trace_spans::SPAN_EVENT_TARGET,
            channel_id = %channel.channel_id,
            timeout_ms = channel.switch_condition.timeout_ms,
            "upstream.timeout"
        );
        record_proxy_metric(product_id, method, "504", mode, "timeout");
        record_upstream_timeout(mode, TimeoutPhase::Read);
        Err(ProxyError::GatewayTimeout(format!(
            "channel wait exceeded switch_condition.timeout_ms for product={product_id}"
        )))
    }
}

fn handle_channel_circuit_result(
    result: CircuitResult<UpstreamResponse>,
    channel: &GatewayCandidateChannel,
    product_id: &str,
    method: &str,
    mode: &str,
    request_id: &str,
) -> Result<UpstreamResponse, ProxyError> {
    match result {
        CircuitResult::Success(response) => {
            let status = response.status.to_string();
            let result_label = classify_proxy_result(response.status, None);
            record_proxy_metric(product_id, method, &status, mode, result_label);
            Ok(response)
        }
        CircuitResult::Rejected {
            reason,
            retry_after,
        } => {
            warn!(
                "[{}] Circuit breaker rejected request for channel [{}] product [{}]: {} (retry_after: {:?})",
                request_id, channel.channel_code, product_id, reason, retry_after
            );
            tracing::info!(
                target: mazbri_core::trace_spans::SPAN_EVENT_TARGET,
                channel_id = %channel.channel_id,
                channel_code = %channel.channel_code,
                reason = %reason,
                "circuit_breaker.rejected"
            );
            record_proxy_metric(product_id, method, "503", mode, "circuit_open");
            Err(ProxyError::CircuitOpen(format!("服务暂时不可用: {reason}")))
        }
        CircuitResult::Failure { error } => {
            warn!(
                "[{}] upstream HTTP/2 request failed for channel [{}] product [{}]: {}",
                request_id, channel.channel_code, product_id, error
            );
            if error.starts_with(UPSTREAM_AUTH_REJECTED_PREFIX) {
                record_proxy_metric(product_id, method, "502", mode, "upstream_auth");
                return Err(ProxyError::UpstreamAuthRejected(error));
            }
            let result_label = if error.contains("contract") || error.contains("contract_id") {
                "config"
            } else {
                "upstream_error"
            };
            record_proxy_metric(product_id, method, "502", mode, result_label);
            Err(ProxyError::BadGateway(error))
        }
    }
}

fn record_proxy_metric(product_id: &str, method: &str, status: &str, mode: &str, result: &str) {
    API_PROXY_REQUESTS_TOTAL
        .with_label_values(&[product_id, method, status, mode, result])
        .inc();
}

/// 异步保存 `CBPro` 兜底快照（脱离租户响应主路径）。
pub(super) fn spawn_store_fallback_snapshot(
    state: &ProxyState,
    req: &GatewayRequest,
    claims: &MazbriClaims,
    request_id: &str,
    response: &UpstreamResponse,
) {
    if !(200..300).contains(&response.status) || state.cbpro_client.is_none() {
        return;
    }
    let state = state.clone();
    let req = req.clone();
    let claims = claims.clone();
    let request_id = request_id.to_string();
    let response = response.clone();
    tokio::spawn(async move {
        store_fallback_snapshot(&state, &req, &claims, &request_id, &response).await;
    });
}

async fn store_fallback_snapshot(
    state: &ProxyState,
    req: &GatewayRequest,
    claims: &MazbriClaims,
    request_id: &str,
    response: &UpstreamResponse,
) {
    let Some(cbpro_client) = state.cbpro_client.as_ref() else {
        return;
    };
    let cache_key = build_fallback_cache_key(&claims.tenant_id, req);
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
