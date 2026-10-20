use super::execution::execute_gateway_request_with_event;
use super::metrics::{API_PROXY_DURATION_SECONDS, classify_error_class};
use super::metrics::{
    ConnectionGuard, GATEWAY_PLATFORM_LOCAL_SECONDS, GATEWAY_UPSTREAM_WAIT_SECONDS,
    extract_user_agent,
};
use super::request_binding::GatewayHttpPath;
use super::response::ProxyError;
use super::serializer::serialize_proxy_response;
use super::validation::{GatewayRequestPayload, sanitize_input};
use crate::auth::claims::MazbriClaims;
use crate::ratelimit::TransferRecord;
use crate::router::AppState;
use axum::{
    Json,
    body::Body,
    extract::{ConnectInfo, Extension, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::{
    AccessHttpFields, AccessLogEvent, AccessNetworkFields, AccessOutcomeFields, AccessRouteFields,
    AccessTimingFields, AccessUpstreamMetaFields, emit_access_log, normalize_upstream_host,
    parse_trace_context, resolve_client_ip, response_flags_for_error, validate_tenant_id,
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

use super::post_response::{PostResponseTaskArgs, content_length, spawn_post_response_tasks};
use super::state::GatewayRequestTiming;

/// `unified_gateway_handler` 依次提取的请求扩展。
type GatewayHandlerExtensions = (
    Extension<MazbriClaims>,
    Extension<GatewayRequestTiming>,
    Extension<GatewayRequestPayload>,
    Extension<GatewayHttpPath>,
    Option<Extension<TenantProductDetail>>,
);

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
    let start = timing.started_at;
    let tenant_validation_started_at = Instant::now();
    let source_ip = resolve_client_ip(&headers, Some(peer_addr));
    let user_agent = extract_user_agent(&headers);
    let _conn_guard = ConnectionGuard::new();
    let tenant_id_validation_started_at = Instant::now();
    let tenant_uuid = match validate_tenant_id(&claims.tenant_id) {
        Ok(uuid) => uuid,
        Err(e) => {
            tracing::warn!(
                tenant_id = % sanitize_input(& claims.tenant_id), request_id = %
                request_id, error = % e,
                "Gateway rejected request: invalid tenant_id in JWT claims"
            );
            return Err(ProxyError::Forbidden(TENANT_ACCESS_DENIED.to_string()));
        }
    };
    let tenant_id_validation_ms =
        mazbri_core::num::millis_u64(tenant_id_validation_started_at.elapsed());
    let (req, crypto_profile, request_was_encrypted, crypto_timing) = state
        .proxy_state
        .crypto_state
        .decrypt_request_value(tenant_uuid, body_payload, &http_path.0)
        .await?;
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
            crypto_timing.decrypt, crypto_parse_ms = crypto_timing.parse,
            codec_version = crypto_profile.codec_version, message =
            "Gateway transport stage completed"
        );
    } else {
        info!(
            target : "gateway.stage", service = "mazbri-gateway", stage_name = "transport",
            stage_ms = crypto_timing.transport_total, request_id = % request_id, tenant_id =
            % claims.tenant_id, tenant_code = % claims.tenant_code, product_id = %
            req.product_id, method = % req.method, path = % req.path, encrypted_request =
            false, transport_profile_ms = crypto_timing.transport_profile,
            transport_total_ms = crypto_timing.transport_total, codec_version =
            crypto_profile.codec_version, message = "Gateway transport stage completed"
        );
    }
    let request_validate_started_at = Instant::now();
    req.validate()?;
    let request_validate_ms = mazbri_core::num::millis_u64(request_validate_started_at.elapsed());

    let mut idempotency_ms = 0u64;
    let mut idempotency_acquired = false;
    if let Some(fingerprint) = req.fingerprint.as_deref() {
        let idempotency_started_at = Instant::now();
        match state
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
                idempotency_ms = mazbri_core::num::millis_u64(idempotency_started_at.elapsed());
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
                // Safe transport retry: return prior success body, skip re-billing.
                let status = StatusCode::from_u16(record.http_status).unwrap_or(StatusCode::OK);
                let mut response = Response::builder().status(status);
                if let Some(ct) = record.content_type.as_deref() {
                    response = response.header(axum::http::header::CONTENT_TYPE, ct);
                } else {
                    response =
                        response.header(axum::http::header::CONTENT_TYPE, "application/json");
                }
                return response.body(Body::from(record.body)).map_err(|error| {
                    ProxyError::InternalError(format!(
                        "failed to build idempotent replay response: {error}"
                    ))
                });
            }
            Ok(crate::crypto::IdempotencyOutcome::Acquired) => {
                idempotency_acquired = true;
            }
            Err(error) => {
                // Fail-open already applied inside check; this branch is for unexpected types.
                error!("[{}] Idempotency check failed: {}", request_id, error);
            }
        }
        idempotency_ms = mazbri_core::num::millis_u64(idempotency_started_at.elapsed());
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
            acquired = idempotency_acquired,
            message = "Gateway idempotency stage completed"
        );
    }
    timing.tenant_validation_ms =
        mazbri_core::num::millis_u64(tenant_validation_started_at.elapsed());
    info!(
        target : "gateway.stage", service = "mazbri-gateway", stage_name = "request_prepare",
        stage_ms = timing.tenant_validation_ms, request_id = % request_id, tenant_id = %
        claims.tenant_id, tenant_code = % claims.tenant_code, product_id = % req.product_id,
        method = % req.method, path = % req.path, encrypted_request = request_was_encrypted,
        tenant_id_validation_ms, request_validate_ms, idempotency_ms,
        tenant_validation_ms = timing.tenant_validation_ms, message =
        "Gateway request preparation stage completed"
    );
    let product_detail_lookup_started_at = Instant::now();
    let tenant_product_detail = match tenant_product_detail {
        Some(Extension(detail)) => Some(detail),
        None => state
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
    let Some(tenant_product_detail) = tenant_product_detail else {
        warn!(
            "[{}] Tenant [{}] attempted to access unauthorized or disabled product: {}",
            request_id,
            claims.tenant_code,
            sanitize_input(&req.product_id)
        );
        return Err(ProxyError::Forbidden(PRODUCT_ACCESS_DENIED.to_string()));
    };
    let fallback_enabled =
        tenant_product_detail.cb_pro_enabled && tenant_product_detail.cb_pro_fallback;
    let upstream_started_at = Instant::now();
    // H-05: try tenant response cache before upstream (fail-open).
    let proxy_cache_key = super::response_cache::build_proxy_cache_key(&req);
    let cache_hit = super::response_cache::try_get_cached_response(
        &state.proxy_state.tarpc_pool,
        &state.proxy_state.services_config.cache,
        &claims.tenant_id,
        &proxy_cache_key,
    )
    .await;
    let (response_body, interaction_event_id, upstream_route, from_cache, from_cbpro_fallback) =
        if let Some(cached) = cache_hit {
            let interaction_event_id = Uuid::new_v4();
            // Attach matched product route so customer-calls shows CB: + channel, not "-".
            let matched_route =
                super::execution::match_primary_route_metadata(&state.proxy_state, &claims, &req);
            info!(
                target: "gateway.stage",
                service = "mazbri-gateway",
                stage_name = "response_cache",
                request_id = %request_id,
                tenant_id = %claims.tenant_id,
                product_id = %req.product_id,
                path = %req.path,
                cache_key = %proxy_cache_key,
                channel_id = matched_route.as_ref().map_or("", |r| r.channel_id.as_str()),
                channel_code = matched_route.as_ref().map_or("", |r| r.channel_code.as_str()),
                message = "Gateway response cache hit"
            );
            (
                cached.into_upstream(),
                interaction_event_id,
                matched_route,
                true,
                false,
            )
        } else {
            match execute_gateway_request_with_event(
                &state.proxy_state,
                &req,
                &claims,
                &request_id,
                fallback_enabled,
                &trace_context,
            )
            .await
            {
                Ok(resp) => (
                    resp.response,
                    resp.interaction_event_id,
                    resp.upstream_route,
                    false,
                    resp.from_cbpro_fallback,
                ),
                Err(e) => {
                    let elapsed = start.elapsed();
                    API_PROXY_DURATION_SECONDS
                        .with_label_values(&[&req.product_id, &req.method, "unknown"])
                        .observe(elapsed.as_secs_f64());
                    return Err(e);
                }
            }
        };
    timing.upstream_ms = mazbri_core::num::millis_u64(upstream_started_at.elapsed());
    info!(
        target : "gateway.stage", service = "mazbri-gateway", stage_name = "upstream",
        stage_ms = timing.upstream_ms, request_id = % request_id, tenant_id = %
        claims.tenant_id, tenant_code = % claims.tenant_code, product_id = % req.product_id,
        method = % req.method, path = % req.path, upstream_service_time_ms =
        timing.upstream_ms, from_cache, message = "Gateway upstream stage completed"
    );
    // Populate cache after successful upstream (not on cache hits).
    if !from_cache {
        let pool = state.proxy_state.tarpc_pool.clone();
        let endpoint = state.proxy_state.services_config.cache.clone();
        let tenant_id = claims.tenant_id.clone();
        let cache_key = proxy_cache_key.clone();
        let path = req.path.clone();
        let to_cache = response_body.clone();
        tokio::spawn(async move {
            super::response_cache::try_put_cached_response(
                &pool, &endpoint, &tenant_id, &cache_key, &to_cache, &path,
            )
            .await;
        });
    }
    let status = response_body.status;
    let status_success = (200..300).contains(&status);
    let http_status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let response_error = if status_success {
        None
    } else {
        Some(response_body.error.clone())
    };
    let response_codec = if crypto_profile.encryption_mode == "full"
        || crypto_profile.encryption_mode == "request"
    {
        crypto_profile.response_codec()
    } else {
        mazbri_core::PayloadCodec::Json
    };
    let response_serialize_started_at = Instant::now();
    let serialized_response = serialize_proxy_response(
        response_codec,
        status,
        &request_id,
        &response_body.body,
        response_error.as_deref(),
        response_body.content_type.as_deref(),
    )?;
    let is_success = serialized_response.success;
    let response_serialize_ms =
        mazbri_core::num::millis_u64(response_serialize_started_at.elapsed());
    info!(
        target : "gateway.stage", service = "mazbri-gateway", stage_name =
        "response_serialize", stage_ms = response_serialize_ms, request_id = % request_id,
        tenant_id = % claims.tenant_id, tenant_code = % claims.tenant_code, product_id = %
        req.product_id, method = % req.method, path = % req.path, response_codec = %
        response_codec.as_str(), response_body_kind = serialized_response.body_kind,
        response_body_encoding = serialized_response.body_encoding, payload_bytes_len =
        response_body.body_bytes_len, encoded_bytes_len = serialized_response.response_bytes,
        message = "Gateway response serialization stage completed"
    );
    let request_size = content_length(&headers) as u64;
    // U3: settle bandwidth token bucket + emit bytes usage without double-counting QPS.
    if let Some(prepaid) = timing.bandwidth_prepaid {
        let bytes_out = serialized_response
            .response_bytes
            .max(response_body.body_bytes_len);
        state
            .proxy_state
            .local_ratelimiter
            .record_transfer(TransferRecord {
                tenant_id: tenant_uuid,
                product_id: Some(req.product_id.clone()),
                api_endpoint: req.path.clone(),
                client_ip: source_ip.clone(),
                policy_version: prepaid.policy_version,
                snapshot_age_ms: prepaid.snapshot_age_ms,
                degraded: prepaid.degraded,
                bytes_in: request_size,
                bytes_out,
                prepaid_request_bytes: prepaid.prepaid_request_bytes,
                bandwidth_enabled: prepaid.bandwidth_enabled,
            });
    }
    let response_encrypt_started_at = Instant::now();
    let encrypted_response = if crypto_profile.encryption_mode == "full" {
        Some(
            state
                .proxy_state
                .crypto_state
                .encrypt_response_bytes(
                    tenant_uuid,
                    &crypto_profile,
                    &serialized_response.bytes,
                    &http_path.0,
                )
                .await?,
        )
    } else {
        None
    };
    let response_encrypt_ms = mazbri_core::num::millis_u64(response_encrypt_started_at.elapsed());
    if crypto_profile.encryption_mode == "full" {
        info!(
            target : "gateway.stage", service = "mazbri-gateway", stage_name =
            "response_encrypt", stage_ms = response_encrypt_ms, request_id = % request_id,
            tenant_id = % claims.tenant_id, tenant_code = % claims.tenant_code, product_id = %
            req.product_id, method = % req.method, path = % req.path, response_encrypt_ms,
            message = "Gateway response encryption stage completed"
        );
    }
    // Complete fingerprint after business success so transport retries replay safely.
    if idempotency_acquired {
        if let Some(fingerprint) = req.fingerprint.as_deref() {
            if is_success {
                let record = crate::crypto::IdempotencyRecord::done(
                    serialized_response.status,
                    serialized_response.bytes.clone(),
                    Some("application/json".to_string()),
                );
                if let Err(error) = state
                    .proxy_state
                    .crypto_state
                    .complete_idempotency(fingerprint, IDEMPOTENCY_TTL_SECS, &record)
                    .await
                {
                    warn!(
                        request_id = %request_id,
                        fingerprint = %fingerprint,
                        error = %error,
                        "Failed to complete idempotency record"
                    );
                }
            }
        }
    }
    spawn_post_response_tasks(
        &state,
        PostResponseTaskArgs {
            request_id: request_id.to_string(),
            tenant_id: tenant_uuid,
            req: req.clone(),
            request_size,
            body: response_body.body.clone(),
            status,
            error: response_error.clone(),
            response_bytes: serialized_response.response_bytes,
            content_type: serialized_response.content_type.clone(),
            tenant_product_detail: Some(tenant_product_detail.clone()),
            tenant_product_id: tenant_product_detail.product_id,
            interaction_event_id,
            service_total_ms: i32::saturating_from(start.elapsed().as_millis()),
            upstream_route: upstream_route.clone(),
            fingerprint: req.fingerprint.clone(),
        },
    );
    let elapsed = start.elapsed();
    let customer_total_ms = mazbri_core::num::millis_u64(elapsed);
    // 仅平台依赖等待（限流/余额/产品），不含上游出站等待
    let dependency_wait_ms =
        timing.ratelimit_ms + timing.balance_check_ms + timing.product_detail_lookup_ms;
    // 网关本地段 = 总耗时 − 上游通道等待（上游业务 RTT 不计入 platform 验收）
    let internal_ms = customer_total_ms.saturating_sub(timing.upstream_ms);
    let upstream_mode = upstream_route
        .as_ref()
        .map_or("unknown", |r| r.upstream_mode.as_str());
    let contract_id = upstream_route
        .as_ref()
        .map_or("", |r| r.contract_id.as_str());
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
            request_id = %request_id,
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
    let response_bytes = serialized_response.response_bytes;
    let route_template = upstream_route
        .as_ref()
        .map_or_else(|| req.path.clone(), |route| route.route_template.clone());
    // Cache / CBPro did not call live upstream — do not imply an upstream host RTT.
    let upstream_host = if from_cache || from_cbpro_fallback {
        None
    } else {
        upstream_route
            .as_ref()
            .and_then(|route| normalize_upstream_host(&route.upstream_url))
    };
    let retry_count = if from_cache || from_cbpro_fallback {
        0
    } else {
        upstream_route
            .as_ref()
            .map(|route| route.attempt_order.saturating_sub(1))
            .unwrap_or_default()
    };
    let error_code = if is_success {
        String::new()
    } else {
        response_error
            .clone()
            .unwrap_or_else(|| "UPSTREAM_ERROR".to_string())
    };
    let error_stage = if is_success {
        String::new()
    } else if from_cbpro_fallback {
        "cbpro_fallback".to_string()
    } else if upstream_route.is_some() {
        "upstream".to_string()
    } else {
        "routing".to_string()
    };
    let response_flags = response_flags_for_error(&error_code, &error_stage, status);
    let error_class = classify_error_class(status, is_success, &error_code);
    let event = AccessLogEvent::builder(state.proxy_state.service_name.as_ref().to_string())
        .trace_context(&trace_context)
        .route(AccessRouteFields {
            tenant_id: Some(claims.tenant_id.clone()),
            tenant_code: Some(claims.tenant_code.clone()),
            product_id: Some(req.product_id.clone()),
            supplier_id: upstream_route
                .as_ref()
                .map(|route| route.supplier_id.clone()),
            supplier_code: upstream_route
                .as_ref()
                .map(|route| route.supplier_code.clone()),
            channel_id: upstream_route
                .as_ref()
                .map(|route| route.channel_id.clone()),
            channel_code: upstream_route
                .as_ref()
                .map(|route| route.channel_code.clone()),
            attempt_order: upstream_route.as_ref().map(|route| route.attempt_order),
        })
        .http(AccessHttpFields {
            method: req.method.clone(),
            protocol: "HTTP/2".to_string(),
            host_header: headers
                .get("host")
                .and_then(|value| value.to_str().ok())
                .map(std::string::ToString::to_string),
            path: req.path.clone(),
            route_template,
            status_code: serialized_response.status,
            request_bytes: request_size,
            response_bytes,
            user_agent: Some(user_agent.clone()),
        })
        .timing(AccessTimingFields {
            duration_ms: customer_total_ms,
            // Cache/CBPro: treat as platform-local serve (no live upstream RTT).
            upstream_service_time_ms: Some(if from_cache || from_cbpro_fallback {
                0
            } else {
                timing.upstream_ms
            }),
            dependency_wait_ms,
            internal_ms,
            post_response_ms: timing.post_response_ms,
        })
        .network(AccessNetworkFields {
            upstream_host,
            upstream_status_code: Some(response_body.status),
            client_ip: Some(source_ip.clone()),
            xff: headers
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok())
                .map(std::string::ToString::to_string),
            cache_hit: from_cache,
            fallback_used: from_cbpro_fallback,
            retry_count,
        })
        .upstream_meta(AccessUpstreamMetaFields {
            upstream_mode: upstream_mode.to_string(),
            contract_id: contract_id.to_string(),
        })
        .outcome(AccessOutcomeFields {
            error_code,
            error_stage,
            response_flags,
            error_class,
        })
        .build();
    emit_access_log("gateway.access", event);
    if let Some(encrypted) = encrypted_response {
        return Ok((http_status, Json(encrypted)).into_response());
    }
    let response = Response::builder()
        .status(http_status)
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(serialized_response.bytes))
        .map_err(|error| {
            ProxyError::InternalError(format!("failed to build proxy response: {error}"))
        })?;
    Ok(response)
}
