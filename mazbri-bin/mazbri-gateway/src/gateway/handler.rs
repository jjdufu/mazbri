use super::balance::{
    BalanceReservationGuard, PrecheckedProductGate, enforce_product_and_balance_gate,
};
use super::execution::execute_gateway_request_with_event;
use super::execution::{UpstreamResponse, UpstreamRouteMetadata};
use super::idempotency::{
    IdempotencyCheckOutcome, IdempotencyInFlightGuard, ReplayedIdempotencyResponse,
};
use super::metrics::{API_PROXY_DURATION_SECONDS, classify_error_class};
use super::metrics::{
    ConnectionGuard, GATEWAY_PLATFORM_LOCAL_SECONDS, GATEWAY_UPSTREAM_WAIT_SECONDS,
    extract_user_agent,
};
use super::post_response::{
    PostResponseTaskArgs, UsageChargeContext, content_length, spawn_post_response_tasks,
};
use super::rejection_log::{AccessLogged, RejectionSlot};
use super::state::GatewayHttpPath;
use super::response::ProxyError;
use super::serializer::{SerializedProxyResponse, serialize_proxy_response};
use super::stage_log::{StageCtx, stage_log};
use super::state::GatewayRequestTiming;
use super::validation::{GatewayRequest, GatewayRequestPayload, sanitize_input};
use crate::auth::ActualRequestBytes;
use crate::auth::claims::MazbriClaims;
use crate::crypto::{TenantCryptoProfile, TenantCryptoTiming};
use crate::ratelimit::{LocalRateLimitDecision, LocalRateLimitInput, TransferRecord};
use crate::router::AppState;
use axum::{
    Json,
    body::Body,
    extract::{ConnectInfo, Extension, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use mazbri_core::crypto::transport::TransportEnvelope;
use mazbri_core::num::SaturatingFrom;
use mazbri_core::{
    AccessBillingFields, AccessHttpFields, AccessLogEvent, AccessNetworkFields,
    AccessOutcomeFields, AccessRouteFields, AccessTimingFields, AccessUpstreamMetaFields,
    TraceContext, classify_outcome, emit_access_log, normalize_upstream_host, validate_tenant_id,
};
use mazbri_product::service::TenantProductDetail;
use rust_decimal::Decimal;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{Instrument as _, warn};
use uuid::Uuid;

const TENANT_ACCESS_DENIED: &str = "无法访问请求的资源";
pub(crate) const PRODUCT_ACCESS_DENIED: &str = "产品访问被拒绝";

/// `unified_gateway_handler` 依次提取的请求扩展。
type GatewayHandlerExtensions = (
    Extension<TraceContext>,
    Extension<MazbriClaims>,
    Extension<GatewayRequestTiming>,
    Extension<GatewayRequestPayload>,
    Extension<GatewayHttpPath>,
    Option<Extension<ActualRequestBytes>>,
    Option<Extension<RejectionSlot>>,
    Option<Extension<PrecheckedProductGate>>,
    Option<Extension<TenantProductDetail>>,
);

/// 处理函数各阶段共享的只读上下文。
struct HandlerCtx<'a> {
    state: &'a AppState,
    claims: &'a MazbriClaims,
    req: &'a GatewayRequest,
    request_id: &'a str,
}

/// 幂等检查结果：重放已完成明文封装或持有 60s 租约继续处理。
enum IdempotencyStage {
    Replay {
        ms: u64,
        replayed: ReplayedIdempotencyResponse,
    },
    Proceed {
        ms: u64,
        guard: Option<IdempotencyInFlightGuard>,
    },
}

/// 缓存命中或上游执行得到的响应。
struct FetchedResponse {
    response: UpstreamResponse,
    interaction_event_id: Uuid,
    upstream_route: Option<UpstreamRouteMetadata>,
    from_cache: bool,
    from_cbpro_fallback: bool,
    cache_key: String,
    cache_lookup_ms: u64,
    cache_lookup_elapsed: Duration,
    /// Full-precision wall duration of `fetch_response` for `OTel` upstream span and hop calculation.
    fetch_wall_duration: Duration,
}

/// 响应完成后用于后置任务、指标与访问日志的输入。
struct CompletedRequest<'a> {
    fetched: &'a FetchedResponse,
    serialized: &'a SerializedProxyResponse,
    response_error: Option<String>,
    request_size: u64,
    timing: &'a mut GatewayRequestTiming,
    headers: &'a HeaderMap,
    source_ip: &'a str,
    user_agent: &'a str,
    trace_context: &'a TraceContext,
    tenant_uuid: Uuid,
    tenant_product_detail: &'a TenantProductDetail,
    idempotency_epoch_id: Option<Uuid>,
    reserved_amount: Decimal,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn unified_gateway_handler(
    State(state): State<AppState>,
    (
        Extension(trace_context),
        Extension(claims),
        Extension(mut timing),
        Extension(body_payload),
        Extension(http_path),
        actual_request_bytes,
        rejection_slot,
        prechecked_gate,
        tenant_product_detail_ext,
    ): GatewayHandlerExtensions,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response, ProxyError> {
    let request_id = Arc::clone(&trace_context.request_id);
    let source_ip = state.ip_extractor.extract_or_unknown(&headers, Some(peer_addr));
    let user_agent = extract_user_agent(&headers);
    let request_size = actual_request_bytes
        .map_or_else(|| content_length(&headers) as u64, |Extension(b)| b.0 as u64);
    let _conn_guard = ConnectionGuard::new();

    let prepare_span = tracing::info_span!(
        "request_prepare",
        tenant_code = %claims.tenant_code,
        product_id = tracing::field::Empty,
    );
    let prepared = run_request_prepare_stage(
        &prepare_span,
        PrepareStageArgs {
            state: &state,
            claims: &claims,
            request_id: &request_id,
            body_payload,
            http_path: &http_path.0,
            rejection_slot: rejection_slot.as_ref().map(|Extension(slot)| slot),
            prechecked_gate,
            tenant_product_detail_ext,
            source_ip: &source_ip,
            request_size,
            timing: &mut timing,
        },
    )
    .instrument(prepare_span.clone())
    .await;
    let prepared = match prepared {
        Ok(Ok(outcome)) => {
            drop(prepare_span);
            outcome
        }
        Ok(Err(early_response)) => {
            drop(prepare_span);
            return Ok(early_response);
        }
        Err(err) => {
            super::stage_log::mark_stage_span_failed(&prepare_span, "request_prepare");
            drop(prepare_span);
            return Err(err);
        }
    };

    dispatch_prepared_outcome(
        prepared,
        DispatchOutcomeArgs {
            state: &state,
            claims: &claims,
            request_id: &request_id,
            http_path: &http_path.0,
            headers: &headers,
            source_ip: &source_ip,
            user_agent: &user_agent,
            trace_context: &trace_context,
            request_size,
            timing: &mut timing,
        },
    )
    .await
}

struct DispatchOutcomeArgs<'a> {
    state: &'a AppState,
    claims: &'a MazbriClaims,
    request_id: &'a str,
    http_path: &'a str,
    headers: &'a HeaderMap,
    source_ip: &'a str,
    user_agent: &'a str,
    trace_context: &'a TraceContext,
    request_size: u64,
    timing: &'a mut GatewayRequestTiming,
}

async fn dispatch_prepared_outcome(
    prepared: PreparedRequestOutcome,
    args: DispatchOutcomeArgs<'_>,
) -> Result<Response, ProxyError> {
    match prepared {
        PreparedRequestOutcome::Replay {
            req,
            tenant_uuid,
            crypto_profile,
            replayed,
        } => {
            let ctx = HandlerCtx {
                state: args.state,
                claims: args.claims,
                req: &req,
                request_id: args.request_id,
            };
            Box::pin(build_idempotency_replay_response(
                &ctx,
                ReplayResponseArgs {
                    tenant_uuid,
                    crypto_profile: &crypto_profile,
                    replayed: &replayed,
                    http_path: args.http_path,
                    timing: args.timing,
                    headers: args.headers,
                    source_ip: args.source_ip,
                    user_agent: args.user_agent,
                    trace_context: args.trace_context,
                    request_size: args.request_size,
                },
            ))
            .await
        }
        PreparedRequestOutcome::Proceed {
            req,
            tenant_uuid,
            crypto_profile,
            tenant_product_detail,
            idem_guard,
            balance_guard,
        } => {
            let ctx = HandlerCtx {
                state: args.state,
                claims: args.claims,
                req: &req,
                request_id: args.request_id,
            };
            Box::pin(execute_live_gateway_pipeline(
                &ctx,
                LivePipelineArgs {
                    tenant_uuid,
                    tenant_product_detail: &tenant_product_detail,
                    crypto_profile: &crypto_profile,
                    http_path: args.http_path,
                    headers: args.headers,
                    source_ip: args.source_ip,
                    user_agent: args.user_agent,
                    trace_context: args.trace_context,
                    request_size: args.request_size,
                    timing: args.timing,
                    idem_guard,
                    balance_guard,
                },
            ))
            .await
        }
    }
}

struct PrepareStageArgs<'a> {
    state: &'a AppState,
    claims: &'a MazbriClaims,
    request_id: &'a str,
    body_payload: GatewayRequestPayload,
    http_path: &'a str,
    rejection_slot: Option<&'a RejectionSlot>,
    prechecked_gate: Option<Extension<PrecheckedProductGate>>,
    tenant_product_detail_ext: Option<Extension<TenantProductDetail>>,
    source_ip: &'a str,
    request_size: u64,
    timing: &'a mut GatewayRequestTiming,
}

enum PreparedRequestOutcome {
    Replay {
        req: GatewayRequest,
        tenant_uuid: Uuid,
        crypto_profile: TenantCryptoProfile,
        replayed: ReplayedIdempotencyResponse,
    },
    Proceed {
        req: GatewayRequest,
        tenant_uuid: Uuid,
        crypto_profile: TenantCryptoProfile,
        tenant_product_detail: Box<TenantProductDetail>,
        idem_guard: Option<IdempotencyInFlightGuard>,
        balance_guard: BalanceReservationGuard,
    },
}

async fn run_request_prepare_stage(
    prepare_span: &tracing::Span,
    args: PrepareStageArgs<'_>,
) -> Result<Result<PreparedRequestOutcome, Response>, ProxyError> {
    let tenant_validation_started_at = Instant::now();
    let tid_started = Instant::now();
    let tenant_uuid = validate_claims_tenant(args.claims, args.request_id)?;
    let tenant_id_validation_ms = mazbri_core::num::millis_u64(tid_started.elapsed());

    let (req, crypto_profile, request_was_encrypted, request_validate_ms) =
        decrypt_and_validate_payload(DecryptPayloadArgs {
            state: args.state,
            claims: args.claims,
            request_id: args.request_id,
            tenant_uuid,
            body_payload: args.body_payload,
            http_path: args.http_path,
            rejection_slot: args.rejection_slot,
        })
        .await?;
    prepare_span.record("product_id", req.product_id.as_str());
    let ctx = HandlerCtx {
        state: args.state,
        claims: args.claims,
        req: &req,
        request_id: args.request_id,
    };
    if let Some(resp) = check_post_decrypt_api_ratelimit(
        &ctx,
        request_was_encrypted,
        tenant_uuid,
        args.source_ip,
        args.request_size,
    ) {
        super::stage_log::mark_stage_span_failed(prepare_span, "request_prepare");
        return Ok(Err(resp));
    }
    let (tenant_product_detail, mut balance_guard) = match resolve_product_and_reserve_balance(
        &ctx,
        tenant_uuid,
        args.prechecked_gate,
        args.tenant_product_detail_ext,
        args.timing,
    )
    .await?
    {
        Ok(pair) => pair,
        Err(resp) => {
            super::stage_log::mark_stage_span_failed(prepare_span, "request_prepare");
            return Ok(Err(resp));
        }
    };
    match idempotency_stage(&ctx, tenant_uuid).await? {
        IdempotencyStage::Replay { ms, replayed } => {
            balance_guard.release_now();
            record_prepare_timing(
                prepare_span,
                &ctx,
                request_was_encrypted,
                tenant_validation_started_at,
                [tenant_id_validation_ms, request_validate_ms, ms],
                args.timing,
            );
            Ok(Ok(PreparedRequestOutcome::Replay {
                req,
                tenant_uuid,
                crypto_profile,
                replayed,
            }))
        }
        IdempotencyStage::Proceed { ms, guard } => {
            record_prepare_timing(
                prepare_span,
                &ctx,
                request_was_encrypted,
                tenant_validation_started_at,
                [tenant_id_validation_ms, request_validate_ms, ms],
                args.timing,
            );
            Ok(Ok(PreparedRequestOutcome::Proceed {
                req,
                tenant_uuid,
                crypto_profile,
                tenant_product_detail: Box::new(tenant_product_detail),
                idem_guard: guard,
                balance_guard,
            }))
        }
    }
}

struct DecryptPayloadArgs<'a> {
    state: &'a AppState,
    claims: &'a MazbriClaims,
    request_id: &'a str,
    tenant_uuid: Uuid,
    body_payload: super::validation::GatewayRequestPayload,
    http_path: &'a str,
    rejection_slot: Option<&'a RejectionSlot>,
}

async fn decrypt_and_validate_payload(
    args: DecryptPayloadArgs<'_>,
) -> Result<(GatewayRequest, TenantCryptoProfile, bool, u64), ProxyError> {
    let transport_span = tracing::info_span!(
        "transport",
        tenant_code = %args.claims.tenant_code,
        product_id = tracing::field::Empty,
    );
    let decrypted = args
        .state
        .proxy_state
        .crypto_state
        .decrypt_request_value(
            args.tenant_uuid,
            args.body_payload,
            args.http_path,
        )
        .instrument(transport_span.clone())
        .await;
    let (req, crypto_profile, request_was_encrypted, crypto_timing) = match decrypted {
        Ok(tuple) => tuple,
        Err(err) => {
            super::stage_log::mark_stage_span_failed(&transport_span, "transport");
            return Err(err);
        }
    };
    transport_span.record("product_id", req.product_id.as_str());
    if request_was_encrypted && let Some(slot) = args.rejection_slot {
        slot.update_inner_request(req.product_id.clone(), req.method.clone(), req.path.clone());
    }
    let ctx = HandlerCtx {
        state: args.state,
        claims: args.claims,
        req: &req,
        request_id: args.request_id,
    };
    log_transport_stage(&transport_span, &ctx, request_was_encrypted, &crypto_timing);
    drop(transport_span);
    let request_validate_started_at = Instant::now();
    req.validate()?;
    let request_validate_ms = mazbri_core::num::millis_u64(request_validate_started_at.elapsed());
    Ok((req, crypto_profile, request_was_encrypted, request_validate_ms))
}

fn record_prepare_timing(
    prepare_span: &tracing::Span,
    ctx: &HandlerCtx<'_>,
    request_was_encrypted: bool,
    started_at: Instant,
    stage_ms: [u64; 3],
    timing: &mut GatewayRequestTiming,
) {
    let elapsed = started_at.elapsed();
    timing.tenant_validation_ms = mazbri_core::num::millis_u64(elapsed);
    log_request_prepare(
        prepare_span,
        ctx,
        request_was_encrypted,
        [
            stage_ms[0],
            stage_ms[1],
            stage_ms[2],
            timing.tenant_validation_ms,
        ],
    );
}

fn check_post_decrypt_api_ratelimit(
    ctx: &HandlerCtx<'_>,
    request_was_encrypted: bool,
    tenant_uuid: Uuid,
    source_ip: &str,
    request_size: u64,
) -> Option<Response> {
    if !request_was_encrypted {
        return None;
    }
    let Some(LocalRateLimitDecision::Deny {
        policy_version,
        retry_after_ms,
        reason,
        snapshot_age_ms,
        degraded,
        quota,
    }) = ctx
        .state
        .proxy_state
        .local_ratelimiter
        .check_api_limit_only(&LocalRateLimitInput {
            tenant_id: tenant_uuid,
            product_id: Some(ctx.req.product_id.clone()),
            api_endpoint: ctx.req.path.clone(),
            client_ip: source_ip.to_string(),
            request_bytes: request_size,
        })
    else {
        return None;
    };
    let stage_ids = super::stage_log::stage_span("ratelimit_inner", Duration::ZERO)
        .product(&ctx.req.product_id)
        .tenant(&ctx.claims.tenant_code)
        .failed()
        .record();
    stage_log!(
        warn,
        "gateway.stage",
        &StageCtx::new(&ctx.request_id, ctx.claims, &ctx.req.product_id),
        &stage_ids,
        "ratelimit",
        0_u64,
        api_endpoint = %ctx.req.path,
        policy_version,
        retry_after_ms,
        reason = %reason,
        snapshot_age_ms,
        degraded,
        source = "local_post_decrypt",
        decision = "deny",
        message = "Gateway post-decrypt API rate limit rejected encrypted request",
    );
    Some(crate::auth::middleware::rate_limit_error_response(
        i32::saturating_from((retry_after_ms / 1000).max(1)),
        &reason,
        quota,
    ))
}

async fn resolve_product_and_reserve_balance(
    ctx: &HandlerCtx<'_>,
    tenant_uuid: Uuid,
    prechecked_gate: Option<Extension<PrecheckedProductGate>>,
    tenant_product_detail_ext: Option<Extension<TenantProductDetail>>,
    timing: &mut GatewayRequestTiming,
) -> Result<Result<(TenantProductDetail, BalanceReservationGuard), Response>, ProxyError> {
    match prechecked_gate {
        Some(Extension(gate)) => {
            let guard = gate.take_guard(tenant_uuid);
            Ok(Ok((gate.detail, guard)))
        }
        None if ctx.state.billing_state.is_some() => {
            let stage_ctx = StageCtx::new(&ctx.request_id, ctx.claims, &ctx.req.product_id);
            match enforce_product_and_balance_gate(
                ctx.state,
                tenant_uuid,
                &ctx.req.product_id,
                stage_ctx,
                timing,
            )
            .await
            {
                Ok(res) => Ok(Ok(res)),
                Err(rejection) => Ok(Err(rejection.into_response())),
            }
        }
        None => {
            let detail =
                resolve_product_detail(ctx, tenant_uuid, tenant_product_detail_ext, timing).await?;
            Ok(Ok((detail, BalanceReservationGuard::none(tenant_uuid))))
        }
    }
}

struct LivePipelineArgs<'a> {
    tenant_uuid: Uuid,
    tenant_product_detail: &'a TenantProductDetail,
    crypto_profile: &'a TenantCryptoProfile,
    http_path: &'a str,
    headers: &'a HeaderMap,
    source_ip: &'a str,
    user_agent: &'a str,
    trace_context: &'a TraceContext,
    request_size: u64,
    timing: &'a mut GatewayRequestTiming,
    idem_guard: Option<super::idempotency::IdempotencyInFlightGuard>,
    balance_guard: BalanceReservationGuard,
}

async fn execute_live_gateway_pipeline(
    ctx: &HandlerCtx<'_>,
    args: LivePipelineArgs<'_>,
) -> Result<Response, ProxyError> {
    let LivePipelineArgs {
        tenant_uuid,
        tenant_product_detail,
        crypto_profile,
        http_path,
        headers,
        source_ip,
        user_agent,
        trace_context,
        request_size,
        timing,
        mut idem_guard,
        mut balance_guard,
    } = args;
    let fallback_enabled =
        tenant_product_detail.cb_pro_enabled && tenant_product_detail.cb_pro_fallback;
    let (cache_allowed, cache_key) = run_route_match_stage(
        ctx,
        tenant_uuid,
        tenant_product_detail.response_cache.enabled,
        timing,
    )
    .await;

    let fetched = match run_upstream_stage(
        ctx,
        fallback_enabled,
        cache_allowed,
        cache_key,
        trace_context,
        timing,
    )
    .await
    {
        Ok(fetched) => fetched,
        Err(err) => {
            release_guards(&mut idem_guard, &mut balance_guard).await;
            return Err(err);
        }
    };
    spawn_cache_populate(ctx, &fetched, cache_allowed, trace_context);
    let status = fetched.response.status;
    let http_status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let response_error = (!(200..300).contains(&status)).then(|| fetched.response.error.clone());
    let serialized = match serialize_stage(ctx, &fetched.response, response_error.as_deref()) {
        Ok(s) => s,
        Err(err) => {
            release_guards(&mut idem_guard, &mut balance_guard).await;
            return Err(err);
        }
    };
    record_bandwidth(
        ctx,
        timing,
        tenant_uuid,
        source_ip,
        request_size,
        &serialized,
        &fetched,
    );
    let encrypted_response =
        match encrypt_stage(ctx, tenant_uuid, crypto_profile, &serialized, http_path).await {
            Ok(enc) => enc,
            Err(err) => {
                release_guards(&mut idem_guard, &mut balance_guard).await;
                return Err(err);
            }
        };

    let idempotency_epoch_id = settle_idempotency_guard(
        idem_guard,
        status,
        &serialized,
        fetched.from_cbpro_fallback,
        &crypto_profile.encryption_mode,
    )
    .await;

    let reserved_amount = balance_guard.take_reserved();
    finish_request(
        ctx,
        &mut CompletedRequest {
            fetched: &fetched,
            serialized: &serialized,
            response_error,
            request_size,
            timing,
            headers,
            source_ip,
            user_agent,
            trace_context,
            tenant_uuid,
            tenant_product_detail,
            idempotency_epoch_id,
            reserved_amount,
        },
    );
    let mut response = build_response(http_status, encrypted_response, serialized.bytes)?;
    response.extensions_mut().insert(AccessLogged);
    Ok(response)
}

async fn run_route_match_stage(
    ctx: &HandlerCtx<'_>,
    tenant_uuid: Uuid,
    snapshot_enabled: bool,
    timing: &mut GatewayRequestTiming,
) -> (bool, String) {
    let started_at = Instant::now();
    let route_span = tracing::info_span!(
        "route_match",
        product_id = %ctx.req.product_id,
        tenant_code = %ctx.claims.tenant_code,
    );
    let out = resolve_request_cache_key(ctx, tenant_uuid, snapshot_enabled)
        .instrument(route_span)
        .await;
    timing.route_match_ms = mazbri_core::num::millis_u64(started_at.elapsed());
    out
}

async fn run_upstream_stage(
    ctx: &HandlerCtx<'_>,
    fallback_enabled: bool,
    cache_allowed: bool,
    cache_key: String,
    trace_context: &TraceContext,
    timing: &mut GatewayRequestTiming,
) -> Result<FetchedResponse, ProxyError> {
    let upstream_span = tracing::info_span!(
        "upstream",
        product_id = %ctx.req.product_id,
        tenant_code = %ctx.claims.tenant_code,
        mode = tracing::field::Empty,
        upstream_wire_ms = tracing::field::Empty,
    );
    let res = fetch_response(
        ctx,
        fallback_enabled,
        cache_allowed,
        cache_key,
        trace_context,
        timing.started_at,
    )
    .instrument(upstream_span.clone())
    .await;
    match res {
        Ok(fetched) => {
            record_fetched_timing(&upstream_span, ctx, &fetched, timing);
            drop(upstream_span);
            Ok(fetched)
        }
        Err(err) => {
            super::stage_log::mark_stage_span_failed(&upstream_span, "upstream");
            drop(upstream_span);
            Err(err)
        }
    }
}

async fn resolve_request_cache_key(
    ctx: &HandlerCtx<'_>,
    tenant_uuid: Uuid,
    snapshot_enabled: bool,
) -> (bool, String) {
    let cache_policy = ctx
        .state
        .proxy_state
        .cache_policy_lookup
        .resolve(tenant_uuid, snapshot_enabled)
        .await;
    let cache_allowed = cache_policy.is_request_cacheable(&ctx.req.method, &ctx.req.path);
    let has_route_override = ctx
        .state
        .proxy_state
        .route_engine
        .load()
        .has_tenant_override(&ctx.claims.tenant_id, &ctx.req.product_id);
    let cache_key = super::response_cache::build_proxy_cache_key(
        &ctx.claims.tenant_id,
        ctx.req,
        cache_policy.tenant_isolated,
        has_route_override,
    );
    (cache_allowed, cache_key)
}

async fn settle_idempotency_guard(
    idem_guard: Option<super::idempotency::IdempotencyInFlightGuard>,
    status: u16,
    serialized: &SerializedProxyResponse,
    from_cbpro_fallback: bool,
    encryption_mode: &str,
) -> Option<Uuid> {
    let mut guard = idem_guard?;
    let epoch_id = guard.epoch_id();
    if (200..300).contains(&status) && serialized.success && !from_cbpro_fallback {
        guard.complete(serialized, encryption_mode).await;
    } else {
        guard.release_now().await;
    }
    Some(epoch_id)
}

async fn release_guards(
    idem_guard: &mut Option<super::idempotency::IdempotencyInFlightGuard>,
    balance_guard: &mut BalanceReservationGuard,
) {
    if let Some(g) = idem_guard {
        g.release_now().await;
    }
    balance_guard.release_now();
}

struct ReplayResponseArgs<'a> {
    tenant_uuid: Uuid,
    crypto_profile: &'a TenantCryptoProfile,
    replayed: &'a ReplayedIdempotencyResponse,
    http_path: &'a str,
    timing: &'a GatewayRequestTiming,
    headers: &'a HeaderMap,
    source_ip: &'a str,
    user_agent: &'a str,
    trace_context: &'a TraceContext,
    request_size: u64,
}

async fn build_idempotency_replay_response(
    ctx: &HandlerCtx<'_>,
    args: ReplayResponseArgs<'_>,
) -> Result<Response, ProxyError> {
    let serialized = &args.replayed.serialized;
    if let Some(prepaid) = args.timing.bandwidth_prepaid {
        ctx.state
            .proxy_state
            .local_ratelimiter
            .record_transfer(TransferRecord {
                tenant_id: args.tenant_uuid,
                product_id: Some(ctx.req.product_id.clone()),
                api_endpoint: ctx.req.path.clone(),
                client_ip: args.source_ip.to_string(),
                policy_version: prepaid.policy_version,
                snapshot_age_ms: prepaid.snapshot_age_ms,
                degraded: prepaid.degraded,
                bytes_in: args.request_size,
                bytes_out: serialized.response_bytes,
                prepaid_request_bytes: prepaid.prepaid_request_bytes,
                bandwidth_enabled: prepaid.bandwidth_enabled,
            });
    }
    let encrypted_response = encrypt_stage(
        ctx,
        args.tenant_uuid,
        args.crypto_profile,
        serialized,
        args.http_path,
    )
    .await?;
    emit_replay_access_log(ctx, &args);
    let http_status = StatusCode::from_u16(serialized.status).unwrap_or(StatusCode::OK);
    let mut response = build_response(http_status, encrypted_response, serialized.bytes.clone())?;
    response.extensions_mut().insert(AccessLogged);
    Ok(response)
}

fn emit_replay_access_log(ctx: &HandlerCtx<'_>, args: &ReplayResponseArgs<'_>) {
    let serialized = &args.replayed.serialized;
    let elapsed_ms = mazbri_core::num::millis_u64(args.timing.started_at.elapsed());
    let dependency_wait_ms = args.timing.ratelimit_ms
        + args.timing.balance_check_ms
        + args.timing.product_detail_lookup_ms;
    let header_str = |name: &str| {
        args.headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(ToString::to_string)
    };
    let outcome = classify_outcome("", "", serialized.status);
    let event = AccessLogEvent::builder(ctx.state.proxy_state.service_name.as_ref().to_string())
        .trace_context(args.trace_context)
        .route(AccessRouteFields {
            tenant_id: Some(ctx.claims.tenant_id.clone()),
            tenant_code: Some(ctx.claims.tenant_code.clone()),
            product_id: Some(ctx.req.product_id.clone()),
            api_key_id: Some(ctx.claims.api_key_id.clone()).filter(|id| !id.is_empty()),
            ..AccessRouteFields::default()
        })
        .http(AccessHttpFields {
            method: ctx.req.method.clone(),
            protocol: "HTTP/2".to_string(),
            host_header: header_str("host"),
            path: ctx.req.path.clone(),
            route_template: ctx.req.path.clone(),
            status_code: serialized.status,
            request_bytes: args.request_size,
            response_bytes: serialized.response_bytes,
            user_agent: Some(args.user_agent.to_string()),
        })
        .timing(AccessTimingFields {
            duration_ms: elapsed_ms,
            upstream_service_time_ms: Some(0),
            dependency_wait_ms,
            internal_ms: elapsed_ms,
            post_response_ms: 0,
        })
        .billing(AccessBillingFields {
            event_id: args.replayed.epoch_id.to_string(),
            billable: false,
            price: String::new(),
            billing_code: 0,
            cost: "0".to_string(),
        })
        .network(AccessNetworkFields {
            upstream_status_code: Some(0),
            client_ip: Some(args.source_ip.to_string()),
            xff: header_str("x-forwarded-for"),
            cache_hit: true,
            fallback_used: false,
            retry_reasons: super::retry_ledger::current_retry_reasons(),
            ..AccessNetworkFields::default()
        })
        .outcome(AccessOutcomeFields {
            error_code: String::new(),
            error_stage: String::new(),
            response_flags: outcome.response_flags.to_string(),
            error_source: outcome.error_source.as_str().to_string(),
            error_class: classify_error_class(serialized.status, serialized.success, ""),
        })
        .build();
    emit_access_log("gateway.access", &event);
}

fn record_fetched_timing(
    upstream_span: &tracing::Span,
    ctx: &HandlerCtx<'_>,
    fetched: &FetchedResponse,
    timing: &mut GatewayRequestTiming,
) {
    timing.cache_lookup_ms = fetched.cache_lookup_ms;
    let live_upstream = !(fetched.from_cache || fetched.from_cbpro_fallback);
    let wire_duration = if live_upstream {
        fetched.response.upstream_wire_duration
    } else {
        Duration::ZERO
    };
    timing.upstream_ms = mazbri_core::num::millis_u64(wire_duration);
    let hop_duration = if live_upstream {
        fetched
            .fetch_wall_duration
            .saturating_sub(fetched.cache_lookup_elapsed)
            .saturating_sub(wire_duration)
    } else {
        Duration::ZERO
    };
    timing.bypass_hop_ms = mazbri_core::num::millis_u64(hop_duration);
    log_upstream_stage(upstream_span, ctx, timing.upstream_ms, fetched);
}

fn log_request_prepare(
    prepare_span: &tracing::Span,
    ctx: &HandlerCtx<'_>,
    request_was_encrypted: bool,
    stage_ms: [u64; 4],
) {
    let (request_id, claims, req) = (ctx.request_id, ctx.claims, ctx.req);
    let [
        tenant_id_validation_ms,
        request_validate_ms,
        idempotency_ms,
        tenant_validation_ms,
    ] = stage_ms;
    let stage_ids = super::stage_log::span_ids_of(prepare_span);
    stage_log!(
        info,
        "gateway.stage",
        &StageCtx::new(&request_id, claims, &req.product_id),
        &stage_ids,
        "request_prepare",
        tenant_validation_ms,
        method = %req.method,
        path = %req.path,
        encrypted_request = request_was_encrypted,
        tenant_id_validation_ms,
        request_validate_ms,
        idempotency_ms,
        tenant_validation_ms,
        message = "Gateway request preparation stage completed",
    );
}

fn log_upstream_stage(
    upstream_span: &tracing::Span,
    ctx: &HandlerCtx<'_>,
    upstream_ms: u64,
    fetched: &FetchedResponse,
) {
    let (request_id, claims, req) = (ctx.request_id, ctx.claims, ctx.req);
    let route = fetched.upstream_route.as_ref();
    let status = fetched.response.status;
    let error_class = classify_error_class(status, (200..300).contains(&status), "");
    let result = if fetched.from_cache {
        "cache"
    } else if fetched.from_cbpro_fallback {
        "cbpro_fallback"
    } else {
        "live"
    };
    upstream_span.record("mode", route.map_or("", |r| r.upstream_mode.as_str()));
    upstream_span.record(
        "upstream_wire_ms",
        i64::try_from(upstream_ms).unwrap_or(i64::MAX),
    );
    if !(200..300).contains(&status) {
        super::stage_log::mark_stage_span_failed(upstream_span, "upstream");
    }
    let stage_ids = super::stage_log::span_ids_of(upstream_span);
    stage_log!(
        info,
        "gateway.stage",
        &StageCtx::new(&request_id, claims, &req.product_id).route(route),
        &stage_ids,
        "upstream",
        upstream_ms,
        method = %req.method,
        path = %req.path,
        upstream_service_time_ms = upstream_ms,
        from_cache = fetched.from_cache,
        upstream_mode = %route.map_or("", |r| r.upstream_mode.as_str()),
        contract_id = %route.map_or("", |r| r.contract_id.as_str()),
        error_class = %error_class,
        result,
        message = "Gateway upstream stage completed",
    );
}

fn build_response(
    http_status: StatusCode,
    encrypted_response: Option<TransportEnvelope>,
    bytes: Vec<u8>,
) -> Result<Response, ProxyError> {
    if let Some(encrypted) = encrypted_response {
        return Ok((http_status, Json(encrypted)).into_response());
    }
    Response::builder()
        .status(http_status)
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(bytes))
        .map_err(|error| {
            ProxyError::InternalError(format!("failed to build proxy response: {error}"))
        })
}

fn validate_claims_tenant(claims: &MazbriClaims, request_id: &str) -> Result<Uuid, ProxyError> {
    validate_tenant_id(&claims.tenant_id).map_err(|e| {
        tracing::warn!(
            tenant_id = %sanitize_input(&claims.tenant_id),
            request_id = %request_id,
            error = %e,
            "Gateway rejected request: invalid tenant_id in JWT claims"
        );
        ProxyError::Forbidden(TENANT_ACCESS_DENIED.to_string())
    })
}

fn log_transport_stage(
    transport_span: &tracing::Span,
    ctx: &HandlerCtx<'_>,
    request_was_encrypted: bool,
    crypto_timing: &TenantCryptoTiming,
) {
    let (request_id, claims, req) = (ctx.request_id, ctx.claims, ctx.req);
    let stage_ids = super::stage_log::span_ids_of(transport_span);
    if request_was_encrypted {
        stage_log!(
            info,
            "gateway.stage",
            &StageCtx::new(&request_id, claims, &req.product_id),
            &stage_ids,
            "transport",
            crypto_timing.transport_total,
            method = %req.method,
            path = %req.path,
            encrypted_request = true,
            transport_profile_ms = crypto_timing.transport_profile,
            transport_total_ms = crypto_timing.transport_total,
            crypto_total_ms = crypto_timing.transport_total,
            crypto_profile_ms = crypto_timing.transport_profile,
            crypto_envelope_ms = crypto_timing.envelope,
            crypto_preflight_ms = crypto_timing.preflight,
            crypto_nonce_ms = crypto_timing.nonce,
            crypto_tenant_key_ms = crypto_timing.tenant_key,
            crypto_tenant_key_db_ms = crypto_timing.tenant_key_db,
            crypto_tenant_key_root_material_ms = crypto_timing.tenant_key_root_material,
            crypto_tenant_key_local_ms = crypto_timing.tenant_key_local,
            crypto_signature_ms = crypto_timing.signature,
            crypto_decrypt_ms = crypto_timing.decrypt,
            crypto_parse_ms = crypto_timing.parse,
            message = "Gateway transport stage completed",
        );
    } else {
        stage_log!(
            info,
            "gateway.stage",
            &StageCtx::new(&request_id, claims, &req.product_id),
            &stage_ids,
            "transport",
            crypto_timing.transport_total,
            method = %req.method,
            path = %req.path,
            encrypted_request = false,
            transport_profile_ms = crypto_timing.transport_profile,
            transport_total_ms = crypto_timing.transport_total,
            message = "Gateway transport stage completed",
        );
    }
}

async fn idempotency_stage(
    ctx: &HandlerCtx<'_>,
    tenant_uuid: Uuid,
) -> Result<IdempotencyStage, ProxyError> {
    let (request_id, claims, req) = (ctx.request_id, ctx.claims, ctx.req);
    let Some(fingerprint) = req.fingerprint.as_deref().filter(|fp| !fp.trim().is_empty()) else {
        return Ok(IdempotencyStage::Proceed {
            ms: 0,
            guard: None,
        });
    };
    let idempotency_started_at = Instant::now();
    let idem_span = tracing::info_span!(
        "idempotency",
        product_id = %req.product_id,
        tenant_code = %claims.tenant_code,
    );
    let outcome = match ctx
        .state
        .proxy_state
        .idempotency
        .check_or_acquire(tenant_uuid, req, fingerprint, request_id)
        .instrument(idem_span.clone())
        .await
    {
        Ok(outcome) => outcome,
        Err(err) => {
            super::stage_log::mark_stage_span_failed(&idem_span, "idempotency");
            return Err(err);
        }
    };
    let idempotency_elapsed = idempotency_started_at.elapsed();
    let idempotency_ms = mazbri_core::num::millis_u64(idempotency_elapsed);
    let stage_ids = super::stage_log::span_ids_of(&idem_span);
    drop(idem_span);
    match outcome {
        IdempotencyCheckOutcome::Replay(replayed) => {
            stage_log!(
                info,
                "gateway.stage",
                &StageCtx::new(&request_id, claims, &req.product_id),
                &stage_ids,
                "idempotency",
                idempotency_ms,
                method = %req.method,
                path = %req.path,
                fingerprint = %fingerprint,
                replay = true,
                message = "Gateway idempotency replay of completed fingerprint",
            );
            Ok(IdempotencyStage::Replay {
                ms: idempotency_ms,
                replayed,
            })
        }
        IdempotencyCheckOutcome::Acquired(guard) => {
            stage_log!(
                info,
                "gateway.stage",
                &StageCtx::new(&request_id, claims, &req.product_id),
                &stage_ids,
                "idempotency",
                idempotency_ms,
                method = %req.method,
                path = %req.path,
                fingerprint = %fingerprint,
                acquired = true,
                message = "Gateway idempotency stage completed",
            );
            Ok(IdempotencyStage::Proceed {
                ms: idempotency_ms,
                guard: Some(guard),
            })
        }
    }
}

async fn resolve_product_detail(
    ctx: &HandlerCtx<'_>,
    tenant_uuid: Uuid,
    from_extension: Option<Extension<TenantProductDetail>>,
    timing: &mut GatewayRequestTiming,
) -> Result<TenantProductDetail, ProxyError> {
    let (request_id, claims, req) = (ctx.request_id, ctx.claims, ctx.req);
    let product_detail_lookup_started_at = Instant::now();
    let prod_span = tracing::info_span!(
        "product_detail_lookup",
        product_id = %req.product_id,
        tenant_code = %claims.tenant_code,
    );
    let tenant_product_detail = match from_extension {
        Some(Extension(detail)) => Some(detail),
        None => match ctx
            .state
            .proxy_state
            .product_service
            .get_tenant_product_config(tenant_uuid, &req.product_id)
            .instrument(prod_span.clone())
            .await
        {
            Ok(detail) => detail,
            Err(e) => {
                super::stage_log::mark_stage_span_failed(&prod_span, "product_detail_lookup");
                return Err(ProxyError::InternalError(format!("查找租户产品配置失败: {e}")));
            }
        },
    };
    let product_detail_lookup_elapsed = product_detail_lookup_started_at.elapsed();
    timing.product_detail_lookup_ms = mazbri_core::num::millis_u64(product_detail_lookup_elapsed);
    if tenant_product_detail.is_none() {
        super::stage_log::mark_stage_span_failed(&prod_span, "product_detail_lookup");
    }
    let stage_ids = super::stage_log::span_ids_of(&prod_span);
    drop(prod_span);
    stage_log!(
        info,
        "gateway.stage",
        &StageCtx::new(&request_id, claims, &req.product_id),
        &stage_ids,
        "product_detail_lookup",
        timing.product_detail_lookup_ms,
        method = %req.method,
        path = %req.path,
        product_detail_lookup_ms = timing.product_detail_lookup_ms,
        message = "Gateway product detail lookup stage completed",
    );
    tenant_product_detail.ok_or_else(|| {
        warn!(
            "[{}] Tenant [{}] attempted to access unauthorized or disabled product: {}",
            request_id,
            claims.tenant_code,
            sanitize_input(&req.product_id)
        );
        ProxyError::Forbidden(PRODUCT_ACCESS_DENIED.to_string())
    })
}

async fn fetch_response(
    ctx: &HandlerCtx<'_>,
    fallback_enabled: bool,
    cache_allowed: bool,
    cache_key: String,
    trace_context: &TraceContext,
    start: Instant,
) -> Result<FetchedResponse, ProxyError> {
    let (state, request_id, claims, req) = (ctx.state, ctx.request_id, ctx.claims, ctx.req);
    let fetch_started = Instant::now();
    let (cache_hit, cache_lookup_elapsed, cache_stage_ids) = if cache_allowed {
        let cache_lookup_started = Instant::now();
        let cache_span = tracing::info_span!(
            "response_cache",
            product_id = %req.product_id,
            tenant_code = %claims.tenant_code,
            hit = tracing::field::Empty,
        );
        let hit = super::response_cache::try_get_cached_response(
            &state.proxy_state.tarpc_pool,
            &state.proxy_state.services_config.cache,
            &claims.tenant_id,
            &cache_key,
            cache_allowed,
        )
        .instrument(cache_span.clone())
        .await;
        cache_span.record("hit", hit.is_some());
        let ids = super::stage_log::span_ids_of(&cache_span);
        drop(cache_span);
        (hit, cache_lookup_started.elapsed(), ids)
    } else {
        (None, Duration::ZERO, super::stage_log::StageSpanIds::default())
    };
    let cache_lookup_ms = mazbri_core::num::millis_u64(cache_lookup_elapsed);
    if let Some(cached) = cache_hit {
        let matched_route =
            super::execution::match_primary_route_metadata(&state.proxy_state, claims, req);
        stage_log!(
            info,
            "gateway.stage",
            &StageCtx::new(&request_id, claims, &req.product_id).route(matched_route.as_ref()),
            &cache_stage_ids,
            "response_cache",
            cache_lookup_ms,
            path = %req.path,
            cache_key = %cache_key,
            message = "Gateway response cache hit",
        );
        let fetch_wall_duration = fetch_started.elapsed();
        return Ok(FetchedResponse {
            response: cached.into_upstream(),
            interaction_event_id: Uuid::now_v7(),
            upstream_route: matched_route,
            from_cache: true,
            from_cbpro_fallback: false,
            cache_key,
            cache_lookup_ms,
            cache_lookup_elapsed,
            fetch_wall_duration,
        });
    }
    match execute_gateway_request_with_event(
        &state.proxy_state,
        req,
        claims,
        request_id,
        fallback_enabled,
        trace_context,
    )
    .await
    {
        Ok(resp) => {
            let fetch_wall_duration = fetch_started.elapsed();
            Ok(FetchedResponse {
                response: resp.response,
                interaction_event_id: resp.interaction_event_id,
                upstream_route: resp.upstream_route,
                from_cache: false,
                from_cbpro_fallback: resp.from_cbpro_fallback,
                cache_key,
                cache_lookup_ms,
                cache_lookup_elapsed,
                fetch_wall_duration,
            })
        }
        Err(e) => {
            API_PROXY_DURATION_SECONDS
                .with_label_values(&[&req.product_id, &req.method, "unknown"])
                .observe(start.elapsed().as_secs_f64());
            Err(e)
        }
    }
}

/// Populate cache after successful live upstream response (never on cache hits or `CBPro` fallback!).
fn spawn_cache_populate(
    ctx: &HandlerCtx<'_>,
    fetched: &FetchedResponse,
    cache_allowed: bool,
    trace_context: &TraceContext,
) {
    if fetched.from_cache
        || fetched.from_cbpro_fallback
        || !super::response_cache::should_attempt_cache_rpc(cache_allowed)
    {
        return;
    }
    let pool = ctx.state.proxy_state.tarpc_pool.clone();
    let endpoint = ctx.state.proxy_state.services_config.cache.clone();
    let tenant_id = ctx.claims.tenant_id.clone();
    let cache_key = fetched.cache_key.clone();
    let path = ctx.req.path.clone();
    let to_cache = fetched.response.clone();
    drop(mazbri_core::panic_policy::spawn_request_subtask(
        "response_cache_populate",
        trace_context.clone(),
        async move {
            super::response_cache::try_put_cached_response(
                &pool,
                &endpoint,
                &tenant_id,
                &cache_key,
                &to_cache,
                &path,
                cache_allowed,
            )
            .await;
        },
    ));
}

fn serialize_stage(
    ctx: &HandlerCtx<'_>,
    response: &UpstreamResponse,
    response_error: Option<&str>,
) -> Result<SerializedProxyResponse, ProxyError> {
    let (request_id, claims, req) = (ctx.request_id, ctx.claims, ctx.req);
    let response_serialize_started_at = Instant::now();
    let ser_span = tracing::info_span!(
        "response_serialize",
        product_id = %req.product_id,
        tenant_code = %claims.tenant_code,
    );
    let serialized_response = match ser_span.in_scope(|| {
        serialize_proxy_response(
            response.status,
            request_id,
            &response.body,
            response_error,
            response.content_type.as_deref(),
        )
    }) {
        Ok(s) => s,
        Err(err) => {
            super::stage_log::mark_stage_span_failed(&ser_span, "response_serialize");
            return Err(err);
        }
    };
    let response_serialize_elapsed = response_serialize_started_at.elapsed();
    let response_serialize_ms = mazbri_core::num::millis_u64(response_serialize_elapsed);
    let stage_ids = super::stage_log::span_ids_of(&ser_span);
    drop(ser_span);
    stage_log!(
        info,
        "gateway.stage",
        &StageCtx::new(&request_id, claims, &req.product_id),
        &stage_ids,
        "response_serialize",
        response_serialize_ms,
        method = %req.method,
        path = %req.path,
        response_body_kind = serialized_response.body_kind,
        response_body_encoding = serialized_response.body_encoding,
        payload_bytes_len = response.body_bytes_len,
        encoded_bytes_len = serialized_response.response_bytes,
        message = "Gateway response serialization stage completed",
    );
    Ok(serialized_response)
}

fn record_bandwidth(
    ctx: &HandlerCtx<'_>,
    timing: &GatewayRequestTiming,
    tenant_uuid: Uuid,
    source_ip: &str,
    request_size: u64,
    serialized: &SerializedProxyResponse,
    fetched: &FetchedResponse,
) {
    let Some(prepaid) = timing.bandwidth_prepaid else {
        return;
    };
    let bytes_out = serialized
        .response_bytes
        .max(fetched.response.body_bytes_len);
    ctx.state
        .proxy_state
        .local_ratelimiter
        .record_transfer(TransferRecord {
            tenant_id: tenant_uuid,
            product_id: Some(ctx.req.product_id.clone()),
            api_endpoint: ctx.req.path.clone(),
            client_ip: source_ip.to_string(),
            policy_version: prepaid.policy_version,
            snapshot_age_ms: prepaid.snapshot_age_ms,
            degraded: prepaid.degraded,
            bytes_in: request_size,
            bytes_out,
            prepaid_request_bytes: prepaid.prepaid_request_bytes,
            bandwidth_enabled: prepaid.bandwidth_enabled,
        });
}

async fn encrypt_stage(
    ctx: &HandlerCtx<'_>,
    tenant_uuid: Uuid,
    crypto_profile: &TenantCryptoProfile,
    serialized: &SerializedProxyResponse,
    http_path: &str,
) -> Result<Option<TransportEnvelope>, ProxyError> {
    if crypto_profile.encryption_mode != "full" {
        return Ok(None);
    }
    let (request_id, claims, req) = (ctx.request_id, ctx.claims, ctx.req);
    let response_encrypt_started_at = Instant::now();
    let enc_span = tracing::info_span!(
        "response_encrypt",
        product_id = %req.product_id,
        tenant_code = %claims.tenant_code,
    );
    let encrypted = match ctx
        .state
        .proxy_state
        .crypto_state
        .encrypt_response_bytes(tenant_uuid, crypto_profile, &serialized.bytes, http_path)
        .instrument(enc_span.clone())
        .await
    {
        Ok(enc) => enc,
        Err(err) => {
            super::stage_log::mark_stage_span_failed(&enc_span, "response_encrypt");
            return Err(err);
        }
    };
    let response_encrypt_elapsed = response_encrypt_started_at.elapsed();
    let response_encrypt_ms = mazbri_core::num::millis_u64(response_encrypt_elapsed);
    let stage_ids = super::stage_log::span_ids_of(&enc_span);
    drop(enc_span);
    stage_log!(
        info,
        "gateway.stage",
        &StageCtx::new(&request_id, claims, &req.product_id),
        &stage_ids,
        "response_encrypt",
        response_encrypt_ms,
        method = %req.method,
        path = %req.path,
        response_encrypt_ms,
        message = "Gateway response encryption stage completed",
    );
    Ok(Some(encrypted))
}

fn finish_request(ctx: &HandlerCtx<'_>, done: &mut CompletedRequest<'_>) {
    let req = ctx.req;
    let fetched = done.fetched;
    let service_total_ms = i32::saturating_from(done.timing.started_at.elapsed().as_millis());
    let billing_enqueue_started = Instant::now();
    let (usage_event, usage_event_produced, billing_fields) =
        prepare_usage_event(ctx, done, service_total_ms);
    done.timing.billing_enqueue_ms =
        mazbri_core::num::millis_u64(billing_enqueue_started.elapsed());
    let stage_ctx = StageCtx::new(&ctx.request_id, ctx.claims, &req.product_id)
        .route(fetched.upstream_route.as_ref());
    spawn_post_response_tasks(
        ctx.state,
        PostResponseTaskArgs {
            request_id: ctx.request_id.to_string(),
            trace_context: done.trace_context.clone(),
            tenant_id: done.tenant_uuid,
            req: req.clone(),
            request_size: done.request_size,
            body: fetched.response.body.clone(),
            status: fetched.response.status,
            error: done.response_error.clone(),
            response_bytes: done.serialized.response_bytes,
            content_type: done.serialized.content_type.clone(),
            tenant_product_detail: Some(done.tenant_product_detail.clone()),
            tenant_product_id: done.tenant_product_detail.product_id,
            interaction_event_id: fetched.interaction_event_id,
            idempotency_epoch_id: done.idempotency_epoch_id,
            from_cbpro_fallback: fetched.from_cbpro_fallback,
            reserved_amount: done.reserved_amount,
            service_total_ms,
            upstream_route: fetched.upstream_route.clone(),
            usage_event,
            usage_event_produced,
            stage_ctx: stage_ctx.clone(),
        },
    );
    let timing = &*done.timing;
    let elapsed = timing.started_at.elapsed();
    let customer_total_ms = mazbri_core::num::millis_u64(elapsed);
    let dependency_wait_ms =
        timing.ratelimit_ms + timing.balance_check_ms + timing.product_detail_lookup_ms;
    let live_upstream = !(fetched.from_cache || fetched.from_cbpro_fallback);
    let wire_duration = if live_upstream {
        fetched.response.upstream_wire_duration
    } else {
        Duration::ZERO
    };
    let internal_duration = elapsed.saturating_sub(wire_duration);
    let internal_ms = mazbri_core::num::millis_u64(internal_duration);
    let upstream_mode = fetched
        .upstream_route
        .as_ref()
        .map_or("unknown", |r| r.upstream_mode.as_str());
    observe_proxy_histograms(req, upstream_mode, elapsed, internal_duration, wire_duration);
    let stage_ids = super::stage_log::current_stage_span_ids();
    if super::stage_log::always_log_stage() {
        stage_log!(
            info,
            "gateway.stage",
            &stage_ctx,
            &stage_ids,
            "platform_summary",
            customer_total_ms,
            duration_ms = customer_total_ms,
            platform_local_ms = internal_ms,
            upstream_wait_ms = timing.upstream_ms,
            dependency_wait_ms,
            balance_check_ms = timing.balance_check_ms,
            ratelimit_ms = timing.ratelimit_ms,
            product_detail_lookup_ms = timing.product_detail_lookup_ms,
            cache_lookup_ms = timing.cache_lookup_ms,
            route_match_ms = timing.route_match_ms,
            channel_rl_wait_ms = timing.channel_rl_wait_ms,
            bypass_hop_ms = timing.bypass_hop_ms,
            billing_enqueue_ms = timing.billing_enqueue_ms,
            nonce_ms = timing.nonce_ms,
            message = "Gateway platform timing summary (upstream_ms = pure wire; internal_ms = total - upstream_ms)",
        );
    }
    emit_gateway_access_log(
        ctx,
        done,
        AccessTimingFields {
            duration_ms: customer_total_ms,
            upstream_service_time_ms: Some(if fetched.from_cache || fetched.from_cbpro_fallback {
                0
            } else {
                timing.upstream_ms
            }),
            dependency_wait_ms,
            internal_ms,
            post_response_ms: timing.post_response_ms,
        },
        billing_fields,
    );
}

fn observe_proxy_histograms(
    req: &GatewayRequest,
    upstream_mode: &str,
    elapsed: Duration,
    internal_duration: Duration,
    wire_duration: Duration,
) {
    API_PROXY_DURATION_SECONDS
        .with_label_values(&[&req.product_id, &req.method, upstream_mode])
        .observe(elapsed.as_secs_f64());
    GATEWAY_PLATFORM_LOCAL_SECONDS
        .with_label_values(&[&req.product_id])
        .observe(internal_duration.as_secs_f64());
    GATEWAY_UPSTREAM_WAIT_SECONDS
        .with_label_values(&[&req.product_id, upstream_mode])
        .observe(wire_duration.as_secs_f64());
}

fn prepare_usage_event(
    ctx: &HandlerCtx<'_>,
    done: &CompletedRequest<'_>,
    service_total_ms: i32,
) -> (
    Option<mazbri_billing::UsageOutboxEvent>,
    bool,
    AccessBillingFields,
) {
    let req = ctx.req;
    let fetched = done.fetched;
    let enqueue_span = tracing::info_span!("billing.enqueue", product_id = %req.product_id);
    let (usage_event, billable) = enqueue_span.in_scope(|| {
        super::post_response::build_usage_event(&UsageChargeContext {
            request_id: ctx.request_id,
            tenant_id: done.tenant_uuid,
            product_detail: Some(done.tenant_product_detail),
            req,
            request_size: done.request_size,
            status: fetched.response.status,
            response_bytes: done.serialized.response_bytes,
            service_total_ms,
            interaction_event_id: fetched.interaction_event_id,
            idempotency_epoch_id: done.idempotency_epoch_id,
            from_cbpro_fallback: fetched.from_cbpro_fallback,
        })
    });
    let billable = billable && ctx.state.billing_state.is_some();
    let usage_event_produced = if billable {
        enqueue_span.in_scope(|| produce_usage_event(ctx, usage_event.clone()))
    } else {
        false
    };
    drop(enqueue_span);
    let billing_fields = AccessBillingFields {
        event_id: usage_event.event_id.to_string(),
        billable,
        price: if billable {
            usage_event.billing_result.amount.to_string()
        } else {
            String::new()
        },
        billing_code: usage_event.billing_code.unwrap_or_default(),
        cost: upstream_cost(fetched),
    };
    (
        billable.then_some(usage_event),
        usage_event_produced,
        billing_fields,
    )
}

fn upstream_cost(fetched: &FetchedResponse) -> String {
    let live_upstream = !(fetched.from_cache || fetched.from_cbpro_fallback);
    match fetched.upstream_route.as_ref() {
        Some(route) if live_upstream => route.cost_price.clone(),
        _ => "0".to_string(),
    }
}

fn produce_usage_event(ctx: &HandlerCtx<'_>, event: mazbri_billing::UsageOutboxEvent) -> bool {
    let Some(producer) = ctx.state.usage_event_producer.as_ref() else {
        return false;
    };
    match producer.try_enqueue(event) {
        Ok(()) => true,
        Err(rejected) => {
            warn!(
                request_id = %ctx.request_id,
                event_id = %rejected.event_id,
                tenant_id = %rejected.tenant_id,
                "Usage event queue full; falling back to outbox path"
            );
            false
        }
    }
}

fn access_error_fields(done: &CompletedRequest<'_>) -> (String, String) {
    let fetched = done.fetched;
    if done.serialized.success {
        return (String::new(), String::new());
    }
    let error_code = done
        .response_error
        .clone()
        .unwrap_or_else(|| "UPSTREAM_ERROR".to_string());
    let error_stage = if fetched.from_cbpro_fallback {
        "cbpro_fallback"
    } else if fetched.upstream_route.is_some() {
        "upstream"
    } else {
        "routing"
    };
    (error_code, error_stage.to_string())
}

fn emit_gateway_access_log(
    ctx: &HandlerCtx<'_>,
    done: &CompletedRequest<'_>,
    timing_fields: AccessTimingFields,
    billing_fields: AccessBillingFields,
) {
    let (claims, req) = (ctx.claims, ctx.req);
    let fetched = done.fetched;
    let route = fetched.upstream_route.as_ref();
    let live_upstream = !(fetched.from_cache || fetched.from_cbpro_fallback);
    let status = fetched.response.status;
    let (error_code, error_stage) = access_error_fields(done);
    let outcome = classify_outcome(&error_code, &error_stage, status);
    let error_class = classify_error_class(status, done.serialized.success, &error_code);
    let header_str = |name: &str| {
        done.headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(std::string::ToString::to_string)
    };
    let event = AccessLogEvent::builder(ctx.state.proxy_state.service_name.as_ref().to_string())
        .trace_context(done.trace_context)
        .route(AccessRouteFields {
            tenant_id: Some(claims.tenant_id.clone()),
            tenant_code: Some(claims.tenant_code.clone()),
            product_id: Some(req.product_id.clone()),
            supplier_id: route.map(|r| r.supplier_id.clone()),
            supplier_code: route.map(|r| r.supplier_code.clone()),
            channel_id: route.map(|r| r.channel_id.clone()),
            channel_code: route.map(|r| r.channel_code.clone()),
            attempt_order: route.map(|r| r.attempt_order),
            api_key_id: Some(claims.api_key_id.clone()).filter(|id| !id.is_empty()),
        })
        .http(AccessHttpFields {
            method: req.method.clone(),
            protocol: "HTTP/2".to_string(),
            host_header: header_str("host"),
            path: req.path.clone(),
            route_template: route.map_or_else(|| req.path.clone(), |r| r.route_template.clone()),
            status_code: done.serialized.status,
            request_bytes: done.request_size,
            response_bytes: done.serialized.response_bytes,
            user_agent: Some(done.user_agent.to_string()),
        })
        .timing(timing_fields)
        .billing(billing_fields)
        .network(AccessNetworkFields {
            upstream_host: route
                .filter(|_| live_upstream)
                .and_then(|r| normalize_upstream_host(&r.upstream_url)),
            upstream_status_code: Some(if live_upstream { status } else { 0 }),
            client_ip: Some(done.source_ip.to_string()),
            xff: header_str("x-forwarded-for"),
            cache_hit: fetched.from_cache,
            fallback_used: fetched.from_cbpro_fallback,
            retry_reasons: super::retry_ledger::current_retry_reasons(),
            upstream_request_id: Some(fetched.response.upstream_request_id.clone())
                .filter(|id| live_upstream && !id.is_empty()),
        })
        .upstream_meta(AccessUpstreamMetaFields {
            upstream_mode: route
                .map_or("unknown", |r| r.upstream_mode.as_str())
                .to_string(),
            contract_id: route.map_or("", |r| r.contract_id.as_str()).to_string(),
        })
        .outcome(AccessOutcomeFields {
            error_code,
            error_stage,
            response_flags: outcome.response_flags.to_string(),
            error_source: outcome.error_source.as_str().to_string(),
            error_class,
        })
        .build();
    emit_access_log("gateway.access", &event);
}
