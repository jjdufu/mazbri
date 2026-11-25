use super::{
    Arc, AttemptStatus, ChannelAttempt, ChannelRateLimitDecision, ChannelRateLimitRequest,
    CircuitBreaker, CircuitBreakerConfig, Deserialize, Duration, GATEWAY_UPSTREAM_ATTEMPTS_TOTAL,
    GatewayCandidateChannel, GatewayRequest, InteractionEvent, MazbriClaims, ProxyError,
    ProxyState, RetryReason, RouteMatchContext, Serialize, TraceContext, UpstreamAttempt, Uuid,
    classify_proxy_result, debug, enqueue_interaction_event, execute_channel_attempt, info,
    record_retry, should_switch, should_switch_status, spawn_store_fallback_snapshot,
    try_cbpro_fallback, warn,
};
use mazbri_core::num::SaturatingFrom;
use tracing::Instrument;

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
    /// Full-precision pure upstream wire duration (`upstream_meta.upstream_wire_us` on bypass).
    pub(crate) upstream_wire_duration: Duration,
    /// 上游回的流水号（bypass `upstream_meta.upstream_request_id`）；无则为空。
    pub(crate) upstream_request_id: String,
}

#[derive(Debug, Clone)]
pub(crate) enum ProxyBody {
    Empty,
    JsonValue(serde_json::Value),
    JsonRaw(Arc<[u8]>),
    JsonRawStr(Arc<str>),
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
    /// adapter name (transparent) | microservice
    pub(crate) upstream_mode: String,
    /// gRPC 契约 ID（见 `upstream_contract::known_contract_ids`）
    pub(crate) contract_id: String,
    /// Channel cost price per upstream call (decimal string).
    pub(crate) cost_price: String,
}

/// Build access-route metadata from a matched candidate channel (zero runtime JSON deserialization).
pub(crate) fn upstream_route_from_channel(
    channel: &GatewayCandidateChannel,
    attempt_order: u32,
) -> UpstreamRouteMetadata {
    let (mode_hint, contract_hint) = channel.prepared.mode_and_contract_hint();
    UpstreamRouteMetadata {
        supplier_id: channel.supplier_id.clone(),
        supplier_code: channel.supplier_code.clone(),
        channel_id: channel.channel_id.clone(),
        channel_code: channel.channel_code.clone(),
        upstream_url: channel.supplier_upstream_url.clone(),
        route_template: channel.path_prefix.clone(),
        attempt_order,
        upstream_mode: mode_hint.to_string(),
        contract_id: contract_hint.to_string(),
        cost_price: channel.cost_price.clone(),
    }
}

fn build_route_match_ctx(claims: &MazbriClaims, req: &GatewayRequest) -> RouteMatchContext {
    let client_version = req.headers.as_ref().and_then(|headers| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-api-version"))
            .map(|(_, v)| v.clone())
    });
    RouteMatchContext {
        tenant_id: claims.tenant_id.clone(),
        product_id: req.product_id.clone(),
        client_version,
    }
}

/// First matched product route channel (for cache / `CBPro` access logs when no live upstream).
pub(crate) fn match_primary_route_metadata(
    state: &ProxyState,
    claims: &MazbriClaims,
    req: &GatewayRequest,
) -> Option<UpstreamRouteMetadata> {
    let match_ctx = build_route_match_ctx(claims, req);
    let current_engine = state.route_engine.load();
    let candidates = current_engine.match_route(&match_ctx)?;
    candidates
        .first()
        .map(|channel| upstream_route_from_channel(channel, 0))
}

pub(crate) async fn get_or_create_breaker(
    state: &ProxyState,
    product_id: &str,
    channel: &GatewayCandidateChannel,
) -> Arc<CircuitBreaker> {
    let breaker_key = format!("channel:{}", channel.channel_id);
    if let Some(breaker) = state.circuit_breakers.get(&breaker_key) {
        return breaker;
    }
    let cb_cfg = &state.circuit_breaker_config;
    let cb_config = match state.product_service.get_product_by_id(product_id).await {
        Ok(Some(product)) => CircuitBreakerConfig {
            failure_threshold: u64::saturating_from(product.cb_failure_threshold),
            success_threshold: u64::saturating_from(product.cb_success_threshold),
            cooldown_duration: Duration::from_secs(u64::saturating_from(product.cb_cooldown_secs)),
            request_timeout: Duration::from_secs(u64::saturating_from(product.timeout_secs)),
            half_open_max_calls: 2,
        },
        _ => CircuitBreakerConfig {
            failure_threshold: u64::from(cb_cfg.failure_threshold),
            success_threshold: u64::from(cb_cfg.success_threshold),
            cooldown_duration: Duration::from_secs(cb_cfg.cooldown_secs),
            request_timeout: Duration::from_secs(cb_cfg.request_timeout_secs),
            half_open_max_calls: 2,
        },
    };
    state.circuit_breakers.get_with_by_ref(&breaker_key, || {
        let breaker = Arc::new(CircuitBreaker::new(breaker_key.clone(), cb_config.clone()));
        info!(
            channel_id = %channel.channel_id,
            channel_code = %channel.channel_code,
            product_id = %product_id,
            failure_threshold = cb_config.failure_threshold,
            success_threshold = cb_config.success_threshold,
            cooldown_secs = cb_config.cooldown_duration.as_secs(),
            "Created new channel-scoped circuit breaker"
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
        let retry_after_secs = self.retry_after_ms.div_ceil(1000).max(1);
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

fn record_attempt_start(event: &mut InteractionEvent, channel: &GatewayCandidateChannel) {
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
    channel: &GatewayCandidateChannel,
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
    record_retry(mode_hint, RetryReason::Switch);
    true
}

/// 记录失败尝试；返回是否按 `switch_condition` 切到下一通道。
fn record_attempt_error(
    event: &mut InteractionEvent,
    e: &ProxyError,
    mode_hint: &str,
    channel: &GatewayCandidateChannel,
) -> bool {
    let status = match e {
        ProxyError::GatewayTimeout(_) => AttemptStatus::Timeout,
        _ => AttemptStatus::Failed(e.to_string()),
    };
    GATEWAY_UPSTREAM_ATTEMPTS_TOTAL
        .with_label_values(&[mode_hint, classify_attempt_error(e)])
        .inc();
    event.mark_last_attempt(status);
    let switch = should_switch(e, &channel.switch_condition);
    if switch {
        record_retry(mode_hint, RetryReason::Switch);
    }
    switch
}

fn classify_attempt_error(e: &ProxyError) -> &'static str {
    let err_s = e.to_string();
    match e {
        ProxyError::GatewayTimeout(_) => "timeout",
        ProxyError::ServiceUnavailable(_) => "overload",
        ProxyError::TooManyRequests { .. } => "ratelimit",
        ProxyError::Forbidden(_) => "authz",
        ProxyError::UpstreamAuthRejected(_) => "upstream_auth",
        ProxyError::CircuitOpen(_) => "circuit_open",
        ProxyError::BadGateway(_)
            if err_s.contains("contract") || err_s.contains("contract_id") =>
        {
            "config"
        }
        ProxyError::BadRequest(_) if err_s.contains("contract") => "config",
        _ => "upstream_error",
    }
}

/// 跨租户通道总 QPS；调用前已确认 `is_enabled() && channel.qps_limit > 0`。
async fn check_channel_rate_limit(
    state: &ProxyState,
    channel: &GatewayCandidateChannel,
    request_id: &str,
) -> Option<ChannelDenied> {
    let span = tracing::info_span!(
        "check_channel_rate_limit",
        channel_id = %channel.channel_id,
        channel_code = %channel.channel_code,
    );
    let decision = state
        .channel_ratelimiter
        .check(ChannelRateLimitRequest {
            channel_id: channel.channel_id.clone(),
            channel_code: channel.channel_code.clone(),
            qps_limit: channel.qps_limit,
            overrides: channel.rate_limit,
        })
        .instrument(span)
        .await;
    match decision {
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
    let interaction_event_id = event.id;

    let match_ctx = build_route_match_ctx(claims, req);
    let current_engine = state.route_engine.load_full();
    let Some(candidates) = current_engine.match_route(&match_ctx) else {
        event.mark_final("no_route");
        enqueue_interaction_event(&state.db, event);
        return Err(ProxyError::NotFound(format!(
            "No route found for product: {}",
            req.product_id
        )));
    };
    let primary_route = candidates
        .first()
        .map(|channel| upstream_route_from_channel(channel, 0));
    let mut saw_non_rate_limit_failure = false;
    let mut last_channel_rate_limit: Option<ChannelDenied> = None;
    let mut last_attempted_route: Option<UpstreamRouteMetadata> = None;
    for (index, channel_arc) in candidates.iter().enumerate() {
        let channel = channel_arc.as_ref();
        last_attempted_route = Some(upstream_route_from_channel(
            channel,
            u32::saturating_from(index + 1),
        ));
        record_attempt_start(&mut event, channel);
        let (mode_hint, _) = channel.prepared.mode_and_contract_hint();

        if state.channel_ratelimiter.is_enabled()
            && channel.qps_limit > 0
            && let Some(denied) = check_channel_rate_limit(state, channel, request_id).await
        {
            GATEWAY_UPSTREAM_ATTEMPTS_TOTAL
                .with_label_values(&[mode_hint, "ratelimit"])
                .inc();
            event.mark_last_attempt(AttemptStatus::Failed(denied.attempt_detail.clone()));
            last_channel_rate_limit = Some(denied);
            continue;
        }

        let attempt = ChannelAttempt::new(channel, index + 1);
        match execute_channel_attempt(state, req, attempt, claims, request_id, trace_context).await
        {
            Ok(response) => {
                if switch_on_status(&mut event, response.status, mode_hint, channel, request_id) {
                    saw_non_rate_limit_failure = true;
                    continue;
                }
                event.mark_last_attempt(AttemptStatus::Success);
                event.mark_final("success");
                enqueue_interaction_event(&state.db, event);
                if fallback_enabled {
                    spawn_store_fallback_snapshot(state, req, claims, request_id, &response);
                }
                let result = classify_proxy_result(response.status, None);
                GATEWAY_UPSTREAM_ATTEMPTS_TOTAL
                    .with_label_values(&[mode_hint, result])
                    .inc();
                return Ok(ProxyExecutionOutcome {
                    response,
                    interaction_event_id,
                    upstream_route: last_attempted_route,
                    from_cbpro_fallback: false,
                });
            }
            Err(e) => {
                if !record_attempt_error(&mut event, &e, mode_hint, channel) {
                    event.mark_final("failed");
                    enqueue_interaction_event(&state.db, event);
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
    finalize_exhausted(ExhaustedContext {
        state,
        req,
        claims,
        request_id,
        fallback_enabled,
        event,
        fallback_route: last_attempted_route.or(primary_route),
        last_channel_rate_limit,
        saw_non_rate_limit_failure,
    })
    .await
}

struct ExhaustedContext<'a> {
    state: &'a ProxyState,
    req: &'a GatewayRequest,
    claims: &'a MazbriClaims,
    request_id: &'a str,
    fallback_enabled: bool,
    event: InteractionEvent,
    fallback_route: Option<UpstreamRouteMetadata>,
    last_channel_rate_limit: Option<ChannelDenied>,
    saw_non_rate_limit_failure: bool,
}

async fn finalize_exhausted(
    mut ctx: ExhaustedContext<'_>,
) -> Result<ProxyExecutionOutcome, ProxyError> {
    let interaction_event_id = ctx.event.id;
    ctx.event.mark_final("exhausted");
    if ctx.fallback_enabled
        && let Some(snapshot) = try_cbpro_fallback(
            ctx.state,
            ctx.req,
            ctx.claims,
            ctx.request_id,
            "exhausted",
        )
        .await
    {
        ctx.event.mark_final("fallback_success");
        enqueue_interaction_event(&ctx.state.db, ctx.event);
        return Ok(ProxyExecutionOutcome {
            response: snapshot,
            interaction_event_id,
            upstream_route: ctx.fallback_route,
            from_cbpro_fallback: true,
        });
    }
    enqueue_interaction_event(&ctx.state.db, ctx.event);
    if let Some(denied) = ctx.last_channel_rate_limit
        && !ctx.saw_non_rate_limit_failure
    {
        return Err(denied.into_error());
    }
    Err(ProxyError::NoHealthyUpstream(
        "All available channels exhausted".to_string(),
    ))
}
