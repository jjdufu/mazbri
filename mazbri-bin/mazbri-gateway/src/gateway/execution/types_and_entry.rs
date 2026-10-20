use super::{
    Arc, AttemptStatus, CandidateChannel, ChannelRateLimitDecision, ChannelRateLimitRequest,
    CircuitBreaker, CircuitBreakerConfig, Deserialize, Duration, GATEWAY_UPSTREAM_ATTEMPTS_TOTAL,
    GatewayRequest, InteractionEvent, MazbriClaims, ProxyError, ProxyState, RouteMatchContext,
    Serialize, TraceContext, UpstreamAttempt, Uuid, classify_proxy_result, debug,
    execute_channel_attempt, info, resolve_mode_and_contract_hint, should_switch,
    should_switch_status, spawn_persist_event_best_effort, store_fallback_snapshot,
    try_cbpro_fallback, warn,
};
use mazbri_core::num::SaturatingFrom;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct FallbackSnapshot {
    pub(super) status: u16,
    pub(super) data: serde_json::Value,
}
#[derive(Debug, Clone)]
pub(crate) struct UpstreamResponse {
    pub(crate) status: u16,
    pub(crate) body: ProxyBody,
    pub(crate) error: String,
    pub(crate) content_type: Option<String>,
    pub(crate) body_bytes_len: u64,
}
#[derive(Debug, Clone)]
pub(crate) enum ProxyBody {
    Empty,
    JsonValue(serde_json::Value),
    JsonRaw(Arc<[u8]>),
    Text(String),
    Binary(Vec<u8>),
}
#[derive(Debug)]
pub(crate) struct ProxyExecutionOutcome {
    pub(crate) response: UpstreamResponse,
    pub(crate) interaction_event_id: Uuid,
    pub(crate) upstream_route: Option<UpstreamRouteMetadata>,
    /// True when body was served from `CBPro` fallback snapshot (not live upstream).
    pub(crate) from_cbpro_fallback: bool,
}
#[derive(Debug, Clone)]
pub(crate) struct UpstreamRouteMetadata {
    pub(crate) supplier_id: String,
    pub(crate) supplier_code: String,
    pub(crate) channel_id: String,
    pub(crate) channel_code: String,
    pub(crate) upstream_url: String,
    pub(crate) route_template: String,
    pub(crate) attempt_order: u32,
    /// transparent | microservice
    pub(crate) upstream_mode: String,
    /// gRPC 契约 ID（见 `upstream_contract::known_contract_ids`）
    pub(crate) contract_id: String,
}

/// Build access-route metadata from a matched candidate channel (no live call required).
pub(crate) fn upstream_route_from_channel(
    channel: &CandidateChannel,
    attempt_order: u32,
) -> UpstreamRouteMetadata {
    let (mode_hint, contract_hint) = resolve_mode_and_contract_hint(channel);
    UpstreamRouteMetadata {
        supplier_id: channel.supplier_id.clone(),
        supplier_code: channel.supplier_code.clone(),
        channel_id: channel.channel_id.clone(),
        channel_code: channel.channel_code.clone(),
        upstream_url: channel.supplier_upstream_url.clone(),
        route_template: channel.path_prefix.clone(),
        attempt_order,
        upstream_mode: mode_hint,
        contract_id: contract_hint,
    }
}

/// First matched product route channel (for cache / `CBPro` access logs when no live upstream).
pub(crate) fn match_primary_route_metadata(
    state: &ProxyState,
    claims: &MazbriClaims,
    req: &GatewayRequest,
) -> Option<UpstreamRouteMetadata> {
    let client_version = req.headers.as_ref().and_then(|headers| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-api-version"))
            .map(|(_, v)| v.clone())
    });
    let match_ctx = RouteMatchContext {
        tenant_id: claims.tenant_id.clone(),
        product_id: req.product_id.clone(),
        client_version,
    };
    let current_engine = state.route_engine.load();
    let candidates = current_engine.match_route(&match_ctx)?;
    candidates
        .first()
        .map(|channel| upstream_route_from_channel(channel, 0))
}
pub(crate) async fn get_or_create_breaker(
    state: &ProxyState,
    product_id: &str,
) -> Arc<CircuitBreaker> {
    if let Some(breaker) = state.circuit_breakers.get(product_id) {
        return breaker;
    }
    let cb_cfg = &state.circuit_breaker_config;
    let cb_config = match state.product_service.get_product_by_id(product_id).await {
        Ok(Some(product)) => CircuitBreakerConfig {
            failure_threshold: u64::saturating_from(product.cb_failure_threshold),
            success_threshold: u64::saturating_from(product.cb_success_threshold),
            cooldown_duration: Duration::from_secs(u64::saturating_from(product.cb_cooldown_secs)),
            request_timeout: Duration::from_secs(u64::saturating_from(product.timeout_secs)),
        },
        _ => CircuitBreakerConfig {
            failure_threshold: u64::from(cb_cfg.failure_threshold),
            success_threshold: u64::from(cb_cfg.success_threshold),
            cooldown_duration: Duration::from_secs(cb_cfg.cooldown_secs),
            request_timeout: Duration::from_secs(cb_cfg.request_timeout_secs),
        },
    };
    state.circuit_breakers.get_with_by_ref(product_id, || {
        let breaker = Arc::new(CircuitBreaker::new(
            format!("product:{product_id}"),
            cb_config.clone(),
        ));
        info!(
            product_id = %product_id,
            failure_threshold = cb_config.failure_threshold,
            success_threshold = cb_config.success_threshold,
            cooldown_secs = cb_config.cooldown_duration.as_secs(),
            "Created new circuit breaker for product"
        );
        breaker
    })
}
/// 通道总 QPS 拒绝信息（用于最终 429 文案）。
struct ChannelDenied {
    retry_after_ms: u64,
    channel_code: String,
    qps_limit: i32,
    reason: &'static str,
    attempt_detail: String,
}

impl ChannelDenied {
    fn into_error(self) -> ProxyError {
        let retry_after_secs = (self.retry_after_ms.div_ceil(1000)).max(1);
        let detail = match self.reason {
            "queue_timeout" => "排队等待超时",
            "queue_full" => "排队已满",
            _ => "请求频率超过通道总限流",
        };
        ProxyError::TooManyRequests {
            message: format!(
                "{detail} (channel={}, qps_limit={}, reason={})",
                self.channel_code, self.qps_limit, self.reason
            ),
            retry_after_secs,
        }
    }
}

fn match_candidates(
    state: &ProxyState,
    req: &GatewayRequest,
    claims: &MazbriClaims,
) -> Option<Vec<CandidateChannel>> {
    let client_version = req.headers.as_ref().and_then(|headers| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-api-version"))
            .map(|(_, v)| v.clone())
    });
    let match_ctx = RouteMatchContext {
        tenant_id: claims.tenant_id.clone(),
        product_id: req.product_id.clone(),
        client_version,
    };
    state.route_engine.load().match_route(&match_ctx)
}

fn record_attempt_start(event: &mut InteractionEvent, channel: &CandidateChannel) {
    event.record_upstream_attempt(
        &channel.channel_id,
        UpstreamAttempt {
            supplier_id: &channel.supplier_id,
            supplier_code: &channel.supplier_code,
            supplier_name: &channel.supplier_name,
            channel_code: &channel.channel_code,
            channel_name: &channel.channel_name,
            endpoint_url: &channel.supplier_upstream_url,
        },
    );
}

/// 非 2xx 且命中 `switch_condition` 时记录失败并返回 `true`。
fn switch_on_status(
    event: &mut InteractionEvent,
    status: u16,
    mode_hint: &str,
    channel: &CandidateChannel,
    request_id: &str,
) -> bool {
    if (200..300).contains(&status) || !should_switch_status(status, &channel.switch_condition) {
        return false;
    }
    warn!(
        "[{}] Channel {} returned HTTP status {}, switching based on condition.",
        request_id, channel.channel_id, status
    );
    event.mark_last_attempt(AttemptStatus::Failed(format!("HTTP status {status}")));
    GATEWAY_UPSTREAM_ATTEMPTS_TOTAL
        .with_label_values(&[mode_hint, classify_proxy_result(status, None)])
        .inc();
    true
}

/// 记录失败尝试；返回是否按 `switch_condition` 切到下一通道。
fn record_attempt_error(
    event: &mut InteractionEvent,
    e: &ProxyError,
    mode_hint: &str,
    channel: &CandidateChannel,
) -> bool {
    let status = match e {
        ProxyError::GatewayTimeout(_) => AttemptStatus::Timeout,
        _ => AttemptStatus::Failed(e.to_string()),
    };
    GATEWAY_UPSTREAM_ATTEMPTS_TOTAL
        .with_label_values(&[mode_hint, classify_attempt_error(e)])
        .inc();
    event.mark_last_attempt(status);
    should_switch(e, &channel.switch_condition)
}

fn classify_attempt_error(e: &ProxyError) -> &'static str {
    let err_s = e.to_string();
    match e {
        ProxyError::GatewayTimeout(_) => "timeout",
        ProxyError::ServiceUnavailable(_) => "overload",
        ProxyError::TooManyRequests { .. } => "ratelimit",
        ProxyError::Forbidden(_) => "authz",
        ProxyError::BadGateway(_)
            if err_s.contains("contract") || err_s.contains("contract_id") =>
        {
            "config"
        }
        ProxyError::BadRequest(_) if err_s.contains("contract") => "config",
        _ => "upstream_error",
    }
}

/// 跨租户通道总 QPS；拒绝时返回 `Some`（已记日志）。
async fn check_channel_rate_limit(
    state: &ProxyState,
    channel: &CandidateChannel,
    request_id: &str,
) -> Option<ChannelDenied> {
    match state
        .channel_ratelimiter
        .check(ChannelRateLimitRequest {
            channel_id: channel.channel_id.clone(),
            channel_code: channel.channel_code.clone(),
            qps_limit: channel.qps_limit,
            channel_config: channel.channel_config.clone(),
        })
        .await
    {
        ChannelRateLimitDecision::Allow {
            queue_wait_ms,
            mode,
        } => {
            if queue_wait_ms > 0 {
                debug!(
                    request_id = %request_id,
                    channel_id = %channel.channel_id,
                    channel_code = %channel.channel_code,
                    queue_wait_ms,
                    mode = %mode,
                    "Channel rate limit queue wait completed"
                );
            }
            None
        }
        ChannelRateLimitDecision::Deny {
            retry_after_ms,
            qps_limit,
            channel_id,
            channel_code,
            reason,
            mode,
        } => {
            warn!(
                request_id = %request_id,
                channel_id = %channel_id,
                channel_code = %channel_code,
                qps_limit,
                retry_after_ms,
                reason,
                mode = %mode,
                "Channel total QPS limit denied; trying next candidate if any"
            );
            Some(ChannelDenied {
                retry_after_ms,
                channel_code,
                qps_limit: channel.qps_limit,
                reason,
                attempt_detail: format!(
                    "channel_qps denied reason={reason} qps_limit={qps_limit} mode={mode}"
                ),
            })
        }
    }
}

pub(crate) async fn execute_gateway_request_with_event(
    state: &ProxyState,
    req: &GatewayRequest,
    claims: &MazbriClaims,
    request_id: &str,
    fallback_enabled: bool,
    trace_context: &TraceContext,
) -> Result<ProxyExecutionOutcome, ProxyError> {
    let mut event = InteractionEvent::new(request_id, &claims.tenant_id, &req.product_id);

    let Some(candidates) = match_candidates(state, req, claims) else {
        event.mark_final("no_route");
        spawn_persist_event_best_effort(state, &event);
        return Err(ProxyError::NotFound(format!(
            "No route found for product: {}",
            req.product_id
        )));
    };
    // Primary matched channel for CBPro/access when live upstream is not used.
    let primary_route = candidates
        .first()
        .map(|channel| upstream_route_from_channel(channel, 0));
    let mut saw_non_rate_limit_failure = false;
    let mut last_channel_rate_limit: Option<ChannelDenied> = None;
    let mut last_attempted_route: Option<UpstreamRouteMetadata> = None;
    for (index, channel) in candidates.into_iter().enumerate() {
        last_attempted_route = Some(upstream_route_from_channel(
            &channel,
            u32::saturating_from(index + 1),
        ));
        record_attempt_start(&mut event, &channel);
        let (mode_hint, _) = resolve_mode_and_contract_hint(&channel);

        // Cross-tenant channel total QPS — independent of tenant ratelimit policy.
        // Default mode=reject (429); optional queue waits under max_wait/max_queue_depth.
        if let Some(denied) = check_channel_rate_limit(state, &channel, request_id).await {
            GATEWAY_UPSTREAM_ATTEMPTS_TOTAL
                .with_label_values(&[mode_hint.as_str(), "ratelimit"])
                .inc();
            event.mark_last_attempt(AttemptStatus::Failed(denied.attempt_detail.clone()));
            last_channel_rate_limit = Some(denied);
            continue;
        }

        match execute_channel_attempt(state, req, &channel, claims, request_id, trace_context).await
        {
            Ok(response) => {
                if switch_on_status(
                    &mut event,
                    response.status,
                    &mode_hint,
                    &channel,
                    request_id,
                ) {
                    saw_non_rate_limit_failure = true;
                    continue;
                }
                event.mark_last_attempt(AttemptStatus::Success);
                event.mark_final("success");
                spawn_persist_event_best_effort(state, &event);
                if fallback_enabled {
                    store_fallback_snapshot(state, req, claims, request_id, &response).await;
                }
                let result = classify_proxy_result(response.status, None);
                GATEWAY_UPSTREAM_ATTEMPTS_TOTAL
                    .with_label_values(&[mode_hint.as_str(), result])
                    .inc();
                // transparent / microservice：access 的 upstream_host 均为业务侧 supplier_upstream_url
                // （transparent 不写 bypass 地址，便于按业务目标 host 排障）
                return Ok(ProxyExecutionOutcome {
                    response,
                    interaction_event_id: event.id,
                    upstream_route: last_attempted_route,
                    from_cbpro_fallback: false,
                });
            }
            Err(e) => {
                if !record_attempt_error(&mut event, &e, &mode_hint, &channel) {
                    event.mark_final("failed");
                    spawn_persist_event_best_effort(state, &event);
                    return Err(e);
                }
                saw_non_rate_limit_failure = true;
                warn!(
                    "[{}] Channel {} failed, switching to next candidate. Error: {}",
                    request_id, channel.channel_id, e
                );
            }
        }
    }
    event.mark_final("exhausted");
    if fallback_enabled
        && let Some(snapshot) =
            try_cbpro_fallback(state, req, claims, request_id, "exhausted").await
    {
        event.mark_final("fallback_success");
        spawn_persist_event_best_effort(state, &event);
        // Attach matched route channel so monitoring shows CB: + channel (not "-").
        return Ok(ProxyExecutionOutcome {
            response: snapshot,
            interaction_event_id: event.id,
            upstream_route: last_attempted_route.or(primary_route),
            from_cbpro_fallback: true,
        });
    }
    spawn_persist_event_best_effort(state, &event);
    if let Some(denied) = last_channel_rate_limit
        && !saw_non_rate_limit_failure
    {
        return Err(denied.into_error());
    }
    Err(ProxyError::BadGateway(
        "All available channels exhausted".to_string(),
    ))
}
