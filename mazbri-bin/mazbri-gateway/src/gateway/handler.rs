use super::execution::execute_gateway_request_with_event;
use super::execution::{UpstreamResponse, UpstreamRouteMetadata};
use super::metrics::{API_PROXY_DURATION_SECONDS, classify_error_class};
use super::metrics::{
    ConnectionGuard, GATEWAY_PLATFORM_LOCAL_SECONDS, GATEWAY_UPSTREAM_WAIT_SECONDS,
    extract_user_agent,
};
use super::request_binding::GatewayHttpPath;
use super::response::ProxyError;
use super::serializer::{SerializedProxyResponse, serialize_proxy_response};
use super::validation::{GatewayRequest, GatewayRequestPayload, sanitize_input};
use crate::auth::claims::MazbriClaims;
use crate::crypto::{TenantCryptoProfile, TenantCryptoTiming};
use crate::ratelimit::TransferRecord;
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
    TraceContext, emit_access_log, normalize_upstream_host, parse_trace_context, resolve_client_ip,
    response_flags_for_error, validate_tenant_id,
};
use mazbri_product::service::TenantProductDetail;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};
use uuid::Uuid;
const TENANT_ACCESS_DENIED: &str = "无法访问请求的资源";
const IDEMPOTENCY_TTL_SECS: u64 = 86_400;
pub(crate) const PRODUCT_ACCESS_DENIED: &str = "产品访问被拒绝";

use super::post_response::{
    PostResponseTaskArgs, UsageChargeContext, content_length, spawn_post_response_tasks,
};
use super::state::GatewayRequestTiming;

/// `unified_gateway_handler` 依次提取的请求扩展。
type GatewayHandlerExtensions = (
    Extension<MazbriClaims>,
    Extension<GatewayRequestTiming>,
    Extension<GatewayRequestPayload>,
    Extension<GatewayHttpPath>,
    Option<Extension<TenantProductDetail>>,
);

/// 处理函数各阶段共享的只读上下文。
struct HandlerCtx<'a> {
    state: &'a AppState,
    claims: &'a MazbriClaims,
    req: &'a GatewayRequest,
    request_id: &'a str,
}

/// 幂等检查结果：重放已完成响应，或继续处理。
enum IdempotencyStage {
    Replay(Response),
    Proceed { ms: u64, acquired: bool },
}

/// 缓存命中或上游执行得到的响应。
struct FetchedResponse {
    response: UpstreamResponse,
    interaction_event_id: Uuid,
    upstream_route: Option<UpstreamRouteMetadata>,
    from_cache: bool,
    from_cbpro_fallback: bool,
    cache_key: String,
}

/// 响应完成后用于后置任务、指标与访问日志的输入。
struct CompletedRequest<'a> {
    fetched: &'a FetchedResponse,
    serialized: &'a SerializedProxyResponse,
    response_error: Option<String>,
    request_size: u64,
    timing: &'a GatewayRequestTiming,
    headers: &'a HeaderMap,
    source_ip: &'a str,
    user_agent: &'a str,
    trace_context: &'a TraceContext,
    tenant_uuid: Uuid,
    tenant_product_detail: &'a TenantProductDetail,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn unified_gateway_handler(
    State(state): State<AppState>,
    // 元组提取器按元素顺序逐个提取，失败时返回该元素自己的拒绝响应，
    // 和拆成多个参数时的提取顺序、拒绝响应完全一致。
    (
        Extension(claims),
        Extension(mut timing),
        Extension(body_payload),
        Extension(http_path),
        tenant_product_detail,
    ): GatewayHandlerExtensions,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response, ProxyError> {
    let trace_context = parse_trace_context(&headers);
    // Arc<str> clone is a refcount bump — avoid allocating a new String for the hot path.
    let request_id = Arc::clone(&trace_context.request_id);
    let tenant_validation_started_at = Instant::now();
    let source_ip = resolve_client_ip(&headers, Some(peer_addr));
    let user_agent = extract_user_agent(&headers);
    let _conn_guard = ConnectionGuard::new();
    let tenant_id_validation_started_at = Instant::now();
    let tenant_uuid = validate_claims_tenant(&claims, &request_id)?;
    let tenant_id_validation_ms =
        mazbri_core::num::millis_u64(tenant_id_validation_started_at.elapsed());
    let (req, crypto_profile, request_was_encrypted, crypto_timing) = state
        .proxy_state
        .crypto_state
        .decrypt_request_value(tenant_uuid, body_payload, &http_path.0)
        .await?;
    let ctx = HandlerCtx {
        state: &state,
        claims: &claims,
        req: &req,
        request_id: &request_id,
    };
    log_transport_stage(&ctx, request_was_encrypted, &crypto_timing);
    let request_validate_started_at = Instant::now();
    req.validate()?;
    let request_validate_ms = mazbri_core::num::millis_u64(request_validate_started_at.elapsed());
    let (idempotency_ms, idempotency_acquired) = match idempotency_stage(&ctx).await? {
        IdempotencyStage::Replay(response) => return Ok(response),
        IdempotencyStage::Proceed { ms, acquired } => (ms, acquired),
    };
    timing.tenant_validation_ms =
        mazbri_core::num::millis_u64(tenant_validation_started_at.elapsed());
    log_request_prepare(
        &ctx,
        request_was_encrypted,
        [
            tenant_id_validation_ms,
            request_validate_ms,
            idempotency_ms,
            timing.tenant_validation_ms,
        ],
    );
    let tenant_product_detail =
        resolve_product_detail(&ctx, tenant_uuid, tenant_product_detail, &mut timing).await?;
    let fallback_enabled =
        tenant_product_detail.cb_pro_enabled && tenant_product_detail.cb_pro_fallback;
    let upstream_started_at = Instant::now();
    let fetched = fetch_response(&ctx, fallback_enabled, &trace_context, timing.started_at).await?;
    timing.upstream_ms = mazbri_core::num::millis_u64(upstream_started_at.elapsed());
    log_upstream_stage(&ctx, timing.upstream_ms, fetched.from_cache);
    spawn_cache_populate(&ctx, &fetched);
    let status = fetched.response.status;
    let http_status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let response_error = (!(200..300).contains(&status)).then(|| fetched.response.error.clone());
    let serialized = serialize_stage(&ctx, &fetched.response, response_error.as_deref())?;
    let request_size = content_length(&headers) as u64;
    record_bandwidth(
        &ctx,
        &timing,
        tenant_uuid,
        &source_ip,
        request_size,
        &serialized,
        &fetched,
    );
    let encrypted_response = encrypt_stage(
        &ctx,
        tenant_uuid,
        &crypto_profile,
        &serialized,
        &http_path.0,
    )
    .await?;
    if idempotency_acquired && serialized.success {
        complete_idempotency(&ctx, &serialized).await;
    }
    finish_request(
        &ctx,
        &CompletedRequest {
            fetched: &fetched,
            serialized: &serialized,
            response_error,
            request_size,
            timing: &timing,
            headers: &headers,
            source_ip: &source_ip,
            user_agent: &user_agent,
            trace_context: &trace_context,
            tenant_uuid,
            tenant_product_detail: &tenant_product_detail,
        },
    )
    .await;
    build_response(http_status, encrypted_response, serialized.bytes)
}

fn log_request_prepare(ctx: &HandlerCtx<'_>, request_was_encrypted: bool, stage_ms: [u64; 4]) {
    let (request_id, claims, req) = (ctx.request_id, ctx.claims, ctx.req);
    let [
        tenant_id_validation_ms,
        request_validate_ms,
        idempotency_ms,
        tenant_validation_ms,
    ] = stage_ms;
    info!(
        target : "gateway.stage", service = "mazbri-gateway", stage_name = "request_prepare",
        stage_ms = tenant_validation_ms, request_id = % request_id, tenant_id = %
        claims.tenant_id, tenant_code = % claims.tenant_code, product_id = % req.product_id,
        method = % req.method, path = % req.path, encrypted_request = request_was_encrypted,
        tenant_id_validation_ms, request_validate_ms, idempotency_ms,
        tenant_validation_ms, message =
        "Gateway request preparation stage completed"
    );
}

fn log_upstream_stage(ctx: &HandlerCtx<'_>, upstream_ms: u64, from_cache: bool) {
    let (request_id, claims, req) = (ctx.request_id, ctx.claims, ctx.req);
    info!(
        target : "gateway.stage", service = "mazbri-gateway", stage_name = "upstream",
        stage_ms = upstream_ms, request_id = % request_id, tenant_id = %
        claims.tenant_id, tenant_code = % claims.tenant_code, product_id = % req.product_id,
        method = % req.method, path = % req.path, upstream_service_time_ms =
        upstream_ms, from_cache, message =
        "Gateway upstream stage completed"
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
            tenant_id = % sanitize_input(& claims.tenant_id), request_id = %
            request_id, error = % e,
            "Gateway rejected request: invalid tenant_id in JWT claims"
        );
        ProxyError::Forbidden(TENANT_ACCESS_DENIED.to_string())
    })
}

fn log_transport_stage(
    ctx: &HandlerCtx<'_>,
    request_was_encrypted: bool,
    crypto_timing: &TenantCryptoTiming,
) {
    let (request_id, claims, req) = (ctx.request_id, ctx.claims, ctx.req);
    if request_was_encrypted {
        info!(
            target : "gateway.stage", service = "mazbri-gateway", stage_name = "transport",
            stage_ms = crypto_timing.transport_total, request_id = % request_id, tenant_id =
            % claims.tenant_id, tenant_code = % claims.tenant_code, product_id = %
            req.product_id, method = % req.method, path = % req.path, encrypted_request = true,
            transport_profile_ms = crypto_timing.transport_profile, transport_total_ms =
            crypto_timing.transport_total, crypto_total_ms = crypto_timing.transport_total,
            crypto_profile_ms = crypto_timing.transport_profile, crypto_envelope_ms =
            crypto_timing.envelope, crypto_preflight_ms = crypto_timing.preflight,
            crypto_nonce_ms = crypto_timing.nonce, crypto_tenant_key_ms =
            crypto_timing.tenant_key, crypto_tenant_key_db_ms = crypto_timing.tenant_key_db,
            crypto_tenant_key_root_material_ms = crypto_timing.tenant_key_root_material,
            crypto_tenant_key_local_ms = crypto_timing.tenant_key_local,
            crypto_signature_ms = crypto_timing.signature, crypto_decrypt_ms =
            crypto_timing.decrypt, crypto_parse_ms = crypto_timing.parse, message =
            "Gateway transport stage completed"
        );
    } else {
        info!(
            target : "gateway.stage", service = "mazbri-gateway", stage_name = "transport",
            stage_ms = crypto_timing.transport_total, request_id = % request_id, tenant_id =
            % claims.tenant_id, tenant_code = % claims.tenant_code, product_id = %
            req.product_id, method = % req.method, path = % req.path, encrypted_request =
            false, transport_profile_ms = crypto_timing.transport_profile,
            transport_total_ms = crypto_timing.transport_total, message = "Gateway transport stage completed"
        );
    }
}

fn replay_response(record: crate::crypto::IdempotencyRecord) -> Result<Response, ProxyError> {
    // Safe transport retry: return prior success body, skip re-billing.
    let status = StatusCode::from_u16(record.http_status).unwrap_or(StatusCode::OK);
    let content_type = record.content_type.as_deref().unwrap_or("application/json");
    Response::builder()
        .status(status)
        .header(axum::http::header::CONTENT_TYPE, content_type)
        .body(Body::from(record.body))
        .map_err(|error| {
            ProxyError::InternalError(format!(
                "failed to build idempotent replay response: {error}"
            ))
        })
}

async fn idempotency_stage(ctx: &HandlerCtx<'_>) -> Result<IdempotencyStage, ProxyError> {
    let (request_id, claims, req) = (ctx.request_id, ctx.claims, ctx.req);
    let Some(fingerprint) = req.fingerprint.as_deref() else {
        return Ok(IdempotencyStage::Proceed {
            ms: 0,
            acquired: false,
        });
    };
    let idempotency_started_at = Instant::now();
    let acquired = match ctx
        .state
        .proxy_state
        .crypto_state
        .check_idempotency(fingerprint, IDEMPOTENCY_TTL_SECS)
        .await
    {
        Ok(crate::crypto::IdempotencyOutcome::InProgress) => {
            warn!(
                "[{}] Duplicate request still processing fingerprint: {}",
                request_id, fingerprint
            );
            return Err(ProxyError::Conflict(format!(
                "Duplicate request is still processing: {fingerprint}"
            )));
        }
        Ok(crate::crypto::IdempotencyOutcome::Completed(record)) => {
            let idempotency_ms = mazbri_core::num::millis_u64(idempotency_started_at.elapsed());
            info!(
                target: "gateway.stage",
                service = "mazbri-gateway",
                stage_name = "idempotency",
                stage_ms = idempotency_ms,
                request_id = %request_id,
                tenant_id = %claims.tenant_id,
                tenant_code = %claims.tenant_code,
                product_id = %req.product_id,
                method = %req.method,
                path = %req.path,
                fingerprint = %fingerprint,
                replay = true,
                message = "Gateway idempotency replay of completed fingerprint"
            );
            return replay_response(record).map(IdempotencyStage::Replay);
        }
        Ok(crate::crypto::IdempotencyOutcome::Acquired) => true,
        Err(error) => {
            // Fail-closed: idempotency store unavailable → 503 (keyless requests never reach here).
            error!("[{}] Idempotency check failed: {}", request_id, error);
            return Err(error);
        }
    };
    let idempotency_ms = mazbri_core::num::millis_u64(idempotency_started_at.elapsed());
    info!(
        target: "gateway.stage",
        service = "mazbri-gateway",
        stage_name = "idempotency",
        stage_ms = idempotency_ms,
        request_id = %request_id,
        tenant_id = %claims.tenant_id,
        tenant_code = %claims.tenant_code,
        product_id = %req.product_id,
        method = %req.method,
        path = %req.path,
        fingerprint = %fingerprint,
        acquired,
        message = "Gateway idempotency stage completed"
    );
    Ok(IdempotencyStage::Proceed {
        ms: idempotency_ms,
        acquired,
    })
}

async fn resolve_product_detail(
    ctx: &HandlerCtx<'_>,
    tenant_uuid: Uuid,
    from_extension: Option<Extension<TenantProductDetail>>,
    timing: &mut GatewayRequestTiming,
) -> Result<TenantProductDetail, ProxyError> {
    let (request_id, claims, req) = (ctx.request_id, ctx.claims, ctx.req);
    let product_detail_lookup_started_at = Instant::now();
    let tenant_product_detail = match from_extension {
        Some(Extension(detail)) => Some(detail),
        None => ctx
            .state
            .proxy_state
            .product_service
            .get_tenant_product_config(tenant_uuid, &req.product_id)
            .await
            .map_err(|e| -> ProxyError {
                ProxyError::InternalError(format!("查找租户产品配置失败: {e}"))
            })?,
    };
    timing.product_detail_lookup_ms =
        mazbri_core::num::millis_u64(product_detail_lookup_started_at.elapsed());
    info!(
        target : "gateway.stage", service = "mazbri-gateway", stage_name =
        "product_detail_lookup", stage_ms = timing.product_detail_lookup_ms, request_id =
        % request_id, tenant_id = % claims.tenant_id, tenant_code = % claims.tenant_code,
        product_id = % req.product_id, method = % req.method, path = % req.path,
        product_detail_lookup_ms = timing.product_detail_lookup_ms, message =
        "Gateway product detail lookup stage completed"
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
    trace_context: &TraceContext,
    start: Instant,
) -> Result<FetchedResponse, ProxyError> {
    let (state, request_id, claims, req) = (ctx.state, ctx.request_id, ctx.claims, ctx.req);
    // H-05: try tenant response cache before upstream (fail-open).
    let cache_key = super::response_cache::build_proxy_cache_key(req);
    let cache_hit = super::response_cache::try_get_cached_response(
        &state.proxy_state.tarpc_pool,
        &state.proxy_state.services_config.cache,
        &claims.tenant_id,
        &cache_key,
    )
    .await;
    if let Some(cached) = cache_hit {
        // Attach matched product route so customer-calls shows CB: + channel, not "-".
        let matched_route =
            super::execution::match_primary_route_metadata(&state.proxy_state, claims, req);
        info!(
            target: "gateway.stage",
            service = "mazbri-gateway",
            stage_name = "response_cache",
            request_id = %request_id,
            tenant_id = %claims.tenant_id,
            product_id = %req.product_id,
            path = %req.path,
            cache_key = %cache_key,
            channel_id = matched_route.as_ref().map_or("", |r| r.channel_id.as_str()),
            channel_code = matched_route.as_ref().map_or("", |r| r.channel_code.as_str()),
            message = "Gateway response cache hit"
        );
        return Ok(FetchedResponse {
            response: cached.into_upstream(),
            interaction_event_id: Uuid::new_v4(),
            upstream_route: matched_route,
            from_cache: true,
            from_cbpro_fallback: false,
            cache_key,
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
        Ok(resp) => Ok(FetchedResponse {
            response: resp.response,
            interaction_event_id: resp.interaction_event_id,
            upstream_route: resp.upstream_route,
            from_cache: false,
            from_cbpro_fallback: resp.from_cbpro_fallback,
            cache_key,
        }),
        Err(e) => {
            API_PROXY_DURATION_SECONDS
                .with_label_values(&[&req.product_id, &req.method, "unknown"])
                .observe(start.elapsed().as_secs_f64());
            Err(e)
        }
    }
}

/// Populate cache after successful upstream (not on cache hits).
fn spawn_cache_populate(ctx: &HandlerCtx<'_>, fetched: &FetchedResponse) {
    if fetched.from_cache {
        return;
    }
    let pool = ctx.state.proxy_state.tarpc_pool.clone();
    let endpoint = ctx.state.proxy_state.services_config.cache.clone();
    let tenant_id = ctx.claims.tenant_id.clone();
    let cache_key = fetched.cache_key.clone();
    let path = ctx.req.path.clone();
    let to_cache = fetched.response.clone();
    tokio::spawn(async move {
        super::response_cache::try_put_cached_response(
            &pool, &endpoint, &tenant_id, &cache_key, &to_cache, &path,
        )
        .await;
    });
}

fn serialize_stage(
    ctx: &HandlerCtx<'_>,
    response: &UpstreamResponse,
    response_error: Option<&str>,
) -> Result<SerializedProxyResponse, ProxyError> {
    let (request_id, claims, req) = (ctx.request_id, ctx.claims, ctx.req);
    let response_serialize_started_at = Instant::now();
    let serialized_response = serialize_proxy_response(
        response.status,
        request_id,
        &response.body,
        response_error,
        response.content_type.as_deref(),
    )?;
    let response_serialize_ms =
        mazbri_core::num::millis_u64(response_serialize_started_at.elapsed());
    info!(
        target : "gateway.stage", service = "mazbri-gateway", stage_name =
        "response_serialize", stage_ms = response_serialize_ms, request_id = % request_id,
        tenant_id = % claims.tenant_id, tenant_code = % claims.tenant_code, product_id = %
        req.product_id, method = % req.method, path = % req.path,  response_body_kind = serialized_response.body_kind,
        response_body_encoding = serialized_response.body_encoding, payload_bytes_len =
        response.body_bytes_len, encoded_bytes_len = serialized_response.response_bytes,
        message = "Gateway response serialization stage completed"
    );
    Ok(serialized_response)
}

/// U3: settle bandwidth token bucket + emit bytes usage without double-counting QPS.
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
    let encrypted = ctx
        .state
        .proxy_state
        .crypto_state
        .encrypt_response_bytes(tenant_uuid, crypto_profile, &serialized.bytes, http_path)
        .await?;
    let response_encrypt_ms = mazbri_core::num::millis_u64(response_encrypt_started_at.elapsed());
    info!(
        target : "gateway.stage", service = "mazbri-gateway", stage_name =
        "response_encrypt", stage_ms = response_encrypt_ms, request_id = % request_id,
        tenant_id = % claims.tenant_id, tenant_code = % claims.tenant_code, product_id = %
        req.product_id, method = % req.method, path = % req.path, response_encrypt_ms,
        message = "Gateway response encryption stage completed"
    );
    Ok(Some(encrypted))
}

/// Complete fingerprint after business success so transport retries replay safely.
async fn complete_idempotency(ctx: &HandlerCtx<'_>, serialized: &SerializedProxyResponse) {
    let Some(fingerprint) = ctx.req.fingerprint.as_deref() else {
        return;
    };
    let record = crate::crypto::IdempotencyRecord::done(
        serialized.status,
        serialized.bytes.clone(),
        Some("application/json".to_string()),
    );
    if let Err(error) = ctx
        .state
        .proxy_state
        .crypto_state
        .complete_idempotency(fingerprint, IDEMPOTENCY_TTL_SECS, &record)
        .await
    {
        warn!(
            request_id = %ctx.request_id,
            fingerprint = %fingerprint,
            error = %error,
            "Failed to complete idempotency record"
        );
    }
}

async fn finish_request(ctx: &HandlerCtx<'_>, done: &CompletedRequest<'_>) {
    let req = ctx.req;
    let fetched = done.fetched;
    let service_total_ms = i32::saturating_from(done.timing.started_at.elapsed().as_millis());
    let (usage_event, usage_event_produced, billing_fields) =
        prepare_usage_event(ctx, done, service_total_ms).await;
    spawn_post_response_tasks(
        ctx.state,
        PostResponseTaskArgs {
            request_id: ctx.request_id.to_string(),
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
            service_total_ms,
            upstream_route: fetched.upstream_route.clone(),
            fingerprint: req.fingerprint.clone(),
            usage_event,
            usage_event_produced,
        },
    );
    let timing = done.timing;
    let elapsed = timing.started_at.elapsed();
    let customer_total_ms = mazbri_core::num::millis_u64(elapsed);
    // 仅平台依赖等待（限流/余额/产品），不含上游出站等待
    let dependency_wait_ms =
        timing.ratelimit_ms + timing.balance_check_ms + timing.product_detail_lookup_ms;
    // 网关本地段 = 总耗时 − 上游通道等待（上游业务 RTT 不计入 platform 验收）
    let internal_ms = customer_total_ms.saturating_sub(timing.upstream_ms);
    let upstream_mode = fetched
        .upstream_route
        .as_ref()
        .map_or("unknown", |r| r.upstream_mode.as_str());
    API_PROXY_DURATION_SECONDS
        .with_label_values(&[&req.product_id, &req.method, upstream_mode])
        .observe(elapsed.as_secs_f64());
    GATEWAY_PLATFORM_LOCAL_SECONDS
        .with_label_values(&[&req.product_id])
        .observe(mazbri_core::num::u64_to_f64(internal_ms) / 1000.0);
    GATEWAY_UPSTREAM_WAIT_SECONDS
        .with_label_values(&[&req.product_id, upstream_mode])
        .observe(mazbri_core::num::u64_to_f64(timing.upstream_ms) / 1000.0);
    // 平台摘要全量保留，便于算 platform_ms
    if super::stage_log::always_log_stage() {
        info!(
            target: "gateway.stage",
            service = "mazbri-gateway",
            stage_name = "platform_summary",
            request_id = %ctx.request_id,
            product_id = %req.product_id,
            duration_ms = customer_total_ms,
            platform_local_ms = internal_ms,
            upstream_wait_ms = timing.upstream_ms,
            dependency_wait_ms,
            balance_check_ms = timing.balance_check_ms,
            ratelimit_ms = timing.ratelimit_ms,
            product_detail_lookup_ms = timing.product_detail_lookup_ms,
            message = "Gateway platform timing summary (upstream business RTT excluded from platform_local)"
        );
    }
    emit_gateway_access_log(
        ctx,
        done,
        AccessTimingFields {
            duration_ms: customer_total_ms,
            // Cache/CBPro: treat as platform-local serve (no live upstream RTT).
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

/// Builds the request's usage event, produces it to Redpanda when billable (L1) and returns
/// (billable event for the post-response path, produced flag, access-log billing fields).
async fn prepare_usage_event(
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
    let (usage_event, billable) = super::post_response::build_usage_event(&UsageChargeContext {
        request_id: ctx.request_id,
        tenant_id: done.tenant_uuid,
        product_detail: Some(done.tenant_product_detail),
        req,
        request_size: done.request_size,
        status: fetched.response.status,
        response_bytes: done.serialized.response_bytes,
        service_total_ms,
        interaction_event_id: fetched.interaction_event_id,
        fingerprint: req.fingerprint.as_deref(),
    });
    let billable = billable && ctx.state.billing_state.is_some();
    let usage_event_produced = if billable {
        produce_usage_event(ctx, &usage_event).await
    } else {
        false
    };
    let billing_fields = AccessBillingFields {
        event_id: usage_event.event_id.to_string(),
        billable,
        price: if billable {
            usage_event.billing_result.amount.to_string()
        } else {
            String::new()
        },
        billing_code: usage_event.billing_code.unwrap_or_default(),
    };
    (
        billable.then_some(usage_event),
        usage_event_produced,
        billing_fields,
    )
}

/// L1: synchronously produce the usage event to Redpanda (acks=all) before the response.
/// Returns false (caller falls back to the outbox/deferred path) when disabled or on error.
async fn produce_usage_event(
    ctx: &HandlerCtx<'_>,
    event: &mazbri_billing::UsageOutboxEvent,
) -> bool {
    let Some(producer) = ctx.state.usage_event_producer.as_ref() else {
        return false;
    };
    match producer.produce(event).await {
        Ok(_) => true,
        Err(error) => {
            warn!(
                request_id = %ctx.request_id,
                event_id = %event.event_id,
                tenant_id = %event.tenant_id,
                error = %error,
                "Usage event produce failed; falling back to outbox path"
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
    let response_flags = response_flags_for_error(&error_code, &error_stage, status);
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
            // Cache / CBPro did not call live upstream — do not imply an upstream host RTT.
            upstream_host: route
                .filter(|_| live_upstream)
                .and_then(|r| normalize_upstream_host(&r.upstream_url)),
            upstream_status_code: Some(status),
            client_ip: Some(done.source_ip.to_string()),
            xff: header_str("x-forwarded-for"),
            cache_hit: fetched.from_cache,
            fallback_used: fetched.from_cbpro_fallback,
            retry_count: route
                .filter(|_| live_upstream)
                .map(|r| r.attempt_order.saturating_sub(1))
                .unwrap_or_default(),
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
            response_flags,
            error_class,
        })
        .build();
    emit_access_log("gateway.access", &event);
}
