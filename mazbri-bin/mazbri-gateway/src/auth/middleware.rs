use super::claims::MazbriClaims;
use super::jwt_keys::JwtHs256Keyring;
use super::tenant_security::{ActualRequestBytes, TenantAccessError};
use crate::gateway::GatewayRequestTiming;
use crate::gateway::request_binding::{
    GatewayHttpPath, RequestBindingInput, enforce_request_binding, record_jwt_kid,
};
use crate::gateway::stage_log::{StageCtx, stage_log};
use crate::gateway::validation::{GatewayRequest, GatewayRequestPayload, MAX_BODY_SIZE};
use crate::ratelimit::{BandwidthPrepaid, LocalRateLimitDecision, LocalRateLimitInput};
use crate::router::AppState;
use axum::{
    body::{self, Body},
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{Validation, decode, decode_header};
use mazbri_auth::{
    AuthFailureReason, CredentialType, TenantContext, TokenRecorder, record_auth_failure,
};
use mazbri_core::crypto::transport::TransportEnvelope;
use mazbri_core::metrics::{REGISTRY, register_collector_once};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::rate_limited_log::RateLimitedLog;
use mazbri_core::{parse_trace_context, validate_tenant_id};
use prometheus::{IntCounterVec, Opts};
use std::net::SocketAddr;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tracing::{error, warn};

pub static RATELIMIT_FAIL_CLOSED_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new(
            "gateway_ratelimit_fail_closed_total",
            "Total gateway requests rejected when rate limit service is unavailable",
        ),
        &["tenant_id"],
    )
    .expect("valid metric definition for gateway ratelimit fail-closed counter")
});

static UNTRUSTED_FORWARDED_HEADER_LOG: LazyLock<RateLimitedLog> =
    LazyLock::new(|| RateLimitedLog::new(Duration::from_secs(60)));

/// # Errors
///
/// 指标注册失败时返回错误。
pub fn register_auth_middleware_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(RATELIMIT_FAIL_CLOSED_TOTAL.clone()))
}

#[derive(Debug)]
pub enum JwtAuthError {
    InvalidToken,
    InvalidRequestBody,
    InsufficientScope,
    AccessDenied,
}

impl std::fmt::Display for JwtAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsufficientScope => write!(f, "权限不足：缺少 api:write 作用域"),
            Self::AccessDenied => write!(f, "访问被拒绝"),
            Self::InvalidToken | Self::InvalidRequestBody => {
                write!(f, "Token 验证失败")
            }
        }
    }
}

impl IntoResponse for JwtAuthError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::InvalidRequestBody => StatusCode::BAD_REQUEST,
            Self::InsufficientScope | Self::AccessDenied => StatusCode::FORBIDDEN,
            Self::InvalidToken => StatusCode::UNAUTHORIZED,
        };
        let body = serde_json::json!({
            "success": false,
            "error": self.to_string(),
            "status": status.as_u16(),
        });
        let mut response = (status, axum::Json(body)).into_response();
        response
            .extensions_mut()
            .insert(crate::gateway::response::GatewayErrorCode(
                crate::gateway::response::status_error_code(status)
                    .0
                    .to_string(),
            ));
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static(
                    "Bearer realm=\"mazbri\", error=\"invalid_token\"",
                ),
            );
        }
        response
    }
}

#[derive(Clone)]
pub struct AuthState {
    pub keyring: JwtHs256Keyring,
}

#[derive(Debug, Clone)]
pub struct ProxyExecutionContext {
    pub api_endpoint: String,
}

impl AuthState {
    #[must_use]
    pub fn new(keyring: JwtHs256Keyring) -> Self {
        Self { keyring }
    }
}

/// 中间件的拒绝响应。只把 `Response` 装箱以缩小 `Result` 的 Err 体积，
/// `into_response` 原样取回内部响应，状态码、响应头和响应体都不变。
pub struct MiddlewareRejection(Box<Response>);

impl From<Response> for MiddlewareRejection {
    fn from(response: Response) -> Self {
        Self(Box::new(response))
    }
}

impl IntoResponse for MiddlewareRejection {
    fn into_response(self) -> Response {
        *self.0
    }
}

fn reject_jwt(reason: AuthFailureReason) -> Response {
    record_auth_failure(reason);
    JwtAuthError::InvalidToken.into_response()
}

const fn jwt_failure_reason(kind: &jsonwebtoken::errors::ErrorKind) -> AuthFailureReason {
    use jsonwebtoken::errors::ErrorKind as K;
    match kind {
        K::ExpiredSignature | K::ImmatureSignature => AuthFailureReason::Expired,
        K::InvalidSignature => AuthFailureReason::Signature,
        _ => AuthFailureReason::Invalid,
    }
}

/// 校验 `Authorization: Bearer <jwt>`（按 kid 取密钥），返回 claims 与 kid。
fn verify_bearer_jwt(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<(MazbriClaims, String), MiddlewareRejection> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| reject_jwt(AuthFailureReason::Missing))?;
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| reject_jwt(AuthFailureReason::Invalid))?;
    let header = decode_header(token).map_err(|_| {
        record_jwt_kid("missing", "reject");
        reject_jwt(AuthFailureReason::Invalid)
    })?;
    let kid = header.kid.ok_or_else(|| {
        record_jwt_kid("missing", "reject");
        reject_jwt(AuthFailureReason::Invalid)
    })?;
    let decoding_key = state.auth_state.keyring.decoding_key(&kid).ok_or_else(|| {
        record_jwt_kid(&kid, "unknown");
        reject_jwt(AuthFailureReason::Signature)
    })?;
    let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.set_issuer(&["mazbri.com"]);
    let claims = decode::<MazbriClaims>(token, &decoding_key, &validation)
        .map_err(|e| {
            warn!(error = %e, error_kind = ?e.kind(), kid = %kid, "JWT verification failed");
            record_jwt_kid(&kid, "reject");
            reject_jwt(jwt_failure_reason(e.kind()))
        })?
        .claims;
    record_jwt_kid(&kid, "ok");
    TokenRecorder::new("jwt").record_validated();
    Ok((claims, kid))
}

/// auth 阶段：补记 span（全量）并按采样输出 stage 日志。
fn log_auth_stage(
    started_at: Instant,
    request_id: &impl std::fmt::Display,
    claims: &MazbriClaims,
    binding_outcome: &crate::gateway::request_binding::RequestBindingOutcome,
    encrypted_request: bool,
) {
    let auth_elapsed = started_at.elapsed();
    let auth_stage_ms = mazbri_core::num::millis_u64(auth_elapsed);
    let stage_ids = crate::gateway::stage_log::stage_span("auth", auth_elapsed)
        .tenant(&claims.tenant_code)
        .record();
    if crate::gateway::stage_log::should_log_stage_info(auth_stage_ms) {
        stage_log!(
            info,
            "gateway.stage",
            &StageCtx::new(request_id, claims, ""),
            &stage_ids,
            "auth",
            auth_stage_ms,
            auth_mode = binding_outcome.auth_mode,
            kid = binding_outcome.kid.as_deref().unwrap_or(""),
            nonce = binding_outcome.nonce_trunc.as_deref().unwrap_or(""),
            sig_ok = binding_outcome.sig_ok,
            key_lookup_ms = binding_outcome.key_lookup_ms,
            verify_ms = binding_outcome.verify_ms,
            encrypted_request,
            message = "Gateway auth stage completed",
        );
    }
}

async fn enforce_gateway_tenant_security(
    state: &AppState,
    extensions: &axum::http::Extensions,
    claims: &MazbriClaims,
    tenant_uuid: uuid::Uuid,
    client_ip: Option<std::net::IpAddr>,
) -> Result<(), MiddlewareRejection> {
    let Err(access_err) = state
        .tenant_security
        .verify_tenant_and_ip(tenant_uuid, client_ip)
        .await
    else {
        return Ok(());
    };
    record_caller_rejection_scope(extensions, claims);
    match access_err {
        TenantAccessError::NotFound
        | TenantAccessError::Suspended
        | TenantAccessError::IpForbidden(_) => {
            warn!(
                tenant_id = %tenant_uuid,
                client_ip = ?client_ip,
                reason = ?access_err,
                "Gateway rejected request due to tenant status or IP whitelist"
            );
            Err(JwtAuthError::AccessDenied.into_response().into())
        }
        TenantAccessError::Internal(err) => {
            error!(
                tenant_id = %tenant_uuid,
                error = %err,
                "Gateway tenant security lookup failed (fail-closed)"
            );
            Err(crate::gateway::response::ProxyError::ServiceUnavailable(
                "Tenant security verification unavailable".to_string(),
            )
            .into_response()
            .into())
        }
    }
}

async fn bind_gateway_invoke_request(
    state: &AppState,
    parts: &mut axum::http::request::Parts,
    claims: &MazbriClaims,
    tenant_context: TenantContext,
    path_and_query: &str,
    body_bytes: &[u8],
    default_kid: String,
) -> Result<crate::gateway::request_binding::RequestBindingOutcome, MiddlewareRejection> {
    if !claims.can_invoke() {
        record_caller_rejection_scope_with_bytes(
            &parts.extensions,
            claims,
            Some(body_bytes.len() as u64),
        );
        warn!(
            tenant_id = %claims.tenant_id,
            scopes = ?claims.scopes,
            "Gateway invoke denied: JWT missing api:write scope"
        );
        return Err(JwtAuthError::InsufficientScope.into_response().into());
    }
    let tenant_uuid = tenant_context.tenant_id;
    prepare_gateway_invoke_extensions(parts, claims, tenant_context, body_bytes)?;
    let envelope_nonce = match parts.extensions.get::<GatewayRequestPayload>() {
        Some(GatewayRequestPayload::Encrypted(env)) => {
            Some((env.key_version.as_str(), env.nonce.as_str()))
        }
        _ => None,
    };
    let mut binding_outcome = enforce_request_binding(RequestBindingInput {
        crypto: &state.proxy_state.crypto_state,
        tenant_id: tenant_uuid,
        require: state.request_binding.require_request_binding,
        nonce_prefix: &state.request_binding.nonce_redis_prefix,
        method: "POST",
        path_and_query,
        headers: &parts.headers,
        body: body_bytes,
        envelope_nonce,
    })
    .await
    .map_err(IntoResponse::into_response)?;
    if binding_outcome.envelope_nonce_prechecked {
        parts
            .extensions
            .insert(crate::crypto::EnvelopeNoncePrechecked);
    }
    if binding_outcome.kid.is_none() {
        binding_outcome.kid = Some(default_kid);
    }
    Ok(binding_outcome)
}

/// # Errors
///
/// 鉴权、租户活性/IP 白名单或请求体校验失败时返回拒绝响应。
pub async fn jwt_auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, MiddlewareRejection> {
    let started_at = Instant::now();
    let request_id = current_request_id(req.headers());
    let peer_addr = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0);
    let client_ip = state.ip_extractor.extract(req.headers(), peer_addr);
    let (claims, kid) = verify_bearer_jwt(&state, req.headers())?;
    let tenant_uuid = validate_tenant_id(&claims.tenant_id).map_err(|err| {
        warn!(
            tenant_id = %claims.tenant_id,
            error = %err,
            "JWT rejected: invalid tenant_id UUID"
        );
        JwtAuthError::AccessDenied.into_response()
    })?;
    enforce_gateway_tenant_security(&state, req.extensions(), &claims, tenant_uuid, client_ip)
        .await?;

    let tenant_context = TenantContext::new(tenant_uuid, CredentialType::Jwt);
    let (mut parts, body) = req.into_parts();
    parts.extensions.insert(claims.clone());
    parts
        .extensions
        .insert(GatewayRequestTiming::new(started_at));
    let body_bytes = body::to_bytes(body, MAX_BODY_SIZE)
        .await
        .map_err(|_| JwtAuthError::InvalidRequestBody.into_response())?;
    parts
        .extensions
        .insert(ActualRequestBytes(body_bytes.len()));
    let path_and_query = parts.uri.path_and_query().map_or_else(
        || parts.uri.path().to_string(),
        |value| value.as_str().to_string(),
    );
    parts
        .extensions
        .insert(GatewayHttpPath(path_and_query.clone()));
    let binding_outcome = if parts.uri.path() == "/api/v1/gateway" {
        bind_gateway_invoke_request(
            &state,
            &mut parts,
            &claims,
            tenant_context,
            &path_and_query,
            &body_bytes,
            kid,
        )
        .await?
    } else {
        parts.extensions.insert(tenant_context);
        crate::gateway::request_binding::RequestBindingOutcome {
            auth_mode: "bearer",
            kid: Some(kid),
            nonce_trunc: None,
            sig_ok: None,
            key_lookup_ms: None,
            verify_ms: None,
            envelope_nonce_prechecked: false,
        }
    };
    if let Some(timing) = parts.extensions.get_mut::<GatewayRequestTiming>() {
        timing.auth_ms = mazbri_core::num::millis_u64(started_at.elapsed());
    }
    let encrypted_request = matches!(
        parts.extensions.get::<GatewayRequestPayload>(),
        Some(GatewayRequestPayload::Encrypted(_))
    );
    log_auth_stage(
        started_at,
        &request_id,
        &claims,
        &binding_outcome,
        encrypted_request,
    );
    Ok(next
        .run(Request::from_parts(parts, Body::from(body_bytes)))
        .await)
}

fn prepare_gateway_invoke_extensions(
    parts: &mut axum::http::request::Parts,
    claims: &MazbriClaims,
    ctx: TenantContext,
    body_bytes: &[u8],
) -> Result<(), MiddlewareRejection> {
    let payload = parse_gateway_request_payload(body_bytes).map_err(|err| {
        record_caller_rejection_scope_with_bytes(
            &parts.extensions,
            claims,
            Some(body_bytes.len() as u64),
        );
        err.into_response()
    })?;
    record_rejection_scope(&parts.extensions, claims, &payload, body_bytes.len() as u64);
    match &payload {
        GatewayRequestPayload::Plain(gateway_req) => {
            parts
                .extensions
                .insert(ctx.with_product_id(gateway_req.product_id.clone()));
            parts.extensions.insert(ProxyExecutionContext {
                api_endpoint: gateway_req.path.clone(),
            });
        }
        GatewayRequestPayload::Encrypted(_) => {
            parts.extensions.insert(ctx);
            parts.extensions.insert(ProxyExecutionContext {
                api_endpoint: parts.uri.path().to_string(),
            });
        }
    }
    parts.extensions.insert(payload);
    Ok(())
}

fn require_tenant_context(req: &Request) -> Result<TenantContext, MiddlewareRejection> {
    req.extensions().get::<TenantContext>().cloned().ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "success": false,
                "error": "未找到租户上下文",
                "status": StatusCode::UNAUTHORIZED.as_u16(),
            })),
        )
            .into_response()
            .into()
    })
}

/// # Errors
///
/// 触发限流时返回 `429` 拒绝响应。
pub async fn ratelimit_middleware(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    mut req: Request,
    next: Next,
) -> Result<Response, MiddlewareRejection> {
    let tenant_context = require_tenant_context(&req)?;
    let api_endpoint = req.extensions().get::<ProxyExecutionContext>().map_or_else(
        || req.uri().path().to_string(),
        |ctx| ctx.api_endpoint.clone(),
    );
    let client_ip = state
        .ip_extractor
        .extract_or_unknown(req.headers(), Some(peer_addr));
    let request_id = current_request_id(req.headers());
    let request_bytes = req
        .extensions()
        .get::<ActualRequestBytes>()
        .map_or_else(|| content_length_header(req.headers()), |b| b.0 as u64);
    let stage_start = Instant::now();
    let mut timing = req
        .extensions()
        .get::<GatewayRequestTiming>()
        .cloned()
        .unwrap_or_else(|| GatewayRequestTiming::new(stage_start));
    let decision = state
        .proxy_state
        .local_ratelimiter
        .check_outer_admission(&LocalRateLimitInput {
            tenant_id: tenant_context.tenant_id,
            product_id: tenant_context.product_id.clone(),
            api_endpoint: api_endpoint.clone(),
            client_ip: client_ip.clone(),
            request_bytes,
        });
    timing.ratelimit_ms = mazbri_core::num::millis_u64(stage_start.elapsed());
    match decision {
        LocalRateLimitDecision::Allow {
            policy_version,
            snapshot_age_ms,
            degraded,
            prepaid_request_bytes,
            bandwidth_enabled,
            quota,
        } => {
            let prepaid = BandwidthPrepaid {
                policy_version,
                snapshot_age_ms,
                degraded,
                prepaid_request_bytes,
                bandwidth_enabled,
            };
            timing.bandwidth_prepaid = Some(prepaid);
            req.extensions_mut().insert(timing);
            log_ratelimit_allow_stage(
                &req,
                &tenant_context,
                &request_id,
                &api_endpoint,
                stage_start.elapsed(),
                prepaid,
            );
            Ok(with_ratelimit_headers(next.run(req).await, quota))
        }
        LocalRateLimitDecision::Deny {
            policy_version,
            retry_after_ms,
            reason,
            snapshot_age_ms,
            degraded,
            quota,
        } => Err(build_ratelimit_deny_rejection(
            &state,
            &req,
            &tenant_context,
            &RateLimitDenyMeta {
                peer_addr,
                request_id: &request_id,
                api_endpoint: &api_endpoint,
                rl_elapsed: stage_start.elapsed(),
                policy_version,
                retry_after_ms,
                reason: &reason,
                snapshot_age_ms,
                degraded,
                quota,
            },
        )),
    }
}

fn log_ratelimit_allow_stage(
    req: &Request,
    tenant_context: &TenantContext,
    request_id: &str,
    api_endpoint: &str,
    rl_elapsed: std::time::Duration,
    prepaid: BandwidthPrepaid,
) {
    let rl_ms = mazbri_core::num::millis_u64(rl_elapsed);
    let stage_ids = ratelimit_stage_span(tenant_context, rl_elapsed).record();
    if crate::gateway::stage_log::should_log_stage_info(rl_ms) {
        stage_log!(
            info,
            "gateway.stage",
            &StageCtx::from_extensions(
                req.extensions(),
                &request_id,
                &tenant_context.tenant_id,
                tenant_context.product_id.as_deref(),
            ),
            &stage_ids,
            "ratelimit",
            rl_ms,
            api_endpoint = %api_endpoint,
            policy_version = prepaid.policy_version,
            snapshot_age_ms = prepaid.snapshot_age_ms,
            degraded = prepaid.degraded,
            source = "local",
            decision = "allow",
            prepaid_request_bytes = prepaid.prepaid_request_bytes,
            bandwidth_enabled = prepaid.bandwidth_enabled,
            message = "Gateway rate limit stage completed",
        );
    }
}

struct RateLimitDenyMeta<'a> {
    peer_addr: SocketAddr,
    request_id: &'a str,
    api_endpoint: &'a str,
    rl_elapsed: std::time::Duration,
    policy_version: i64,
    retry_after_ms: u64,
    reason: &'a str,
    snapshot_age_ms: u64,
    degraded: bool,
    quota: Option<crate::ratelimit::QuotaState>,
}

fn build_ratelimit_deny_rejection(
    state: &AppState,
    req: &Request,
    tenant_context: &TenantContext,
    deny: &RateLimitDenyMeta<'_>,
) -> MiddlewareRejection {
    if deny.reason == "client_ip_qps"
        && state
            .ip_extractor
            .has_untrusted_forwarded_header(req.headers(), Some(deny.peer_addr))
        && UNTRUSTED_FORWARDED_HEADER_LOG.allow(&deny.peer_addr.ip().to_string())
    {
        warn!(
            peer_ip = %deny.peer_addr.ip(),
            tenant_id = %tenant_context.tenant_id,
            "Client IP rate limit triggered on untrusted proxy peer carrying X-Forwarded-For/X-Real-IP; configure server.trusted_proxies if behind a reverse proxy"
        );
    }
    let rl_ms = mazbri_core::num::millis_u64(deny.rl_elapsed);
    let stage_ids = ratelimit_stage_span(tenant_context, deny.rl_elapsed)
        .failed()
        .record();
    stage_log!(
        warn,
        "gateway.stage",
        &StageCtx::from_extensions(
            req.extensions(),
            &deny.request_id,
            &tenant_context.tenant_id,
            tenant_context.product_id.as_deref(),
        ),
        &stage_ids,
        "ratelimit",
        rl_ms,
        api_endpoint = %deny.api_endpoint,
        policy_version = deny.policy_version,
        retry_after_ms = deny.retry_after_ms,
        reason = %deny.reason,
        snapshot_age_ms = deny.snapshot_age_ms,
        degraded = deny.degraded,
        source = "local",
        decision = "deny",
        message = "Gateway rate limit stage rejected request",
    );
    rate_limit_error_response(
        i32::saturating_from((deny.retry_after_ms / 1000).max(1)),
        deny.reason,
        deny.quota,
    )
    .into()
}

fn ratelimit_stage_span(
    tenant_context: &TenantContext,
    duration: std::time::Duration,
) -> crate::gateway::stage_log::StageSpan {
    let span = crate::gateway::stage_log::stage_span("ratelimit", duration);
    match tenant_context.product_id.as_deref() {
        Some(product_id) => span.product(product_id),
        None => span,
    }
}

fn content_length_header(headers: &axum::http::HeaderMap) -> u64 {
    headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

fn record_caller_rejection_scope(extensions: &axum::http::Extensions, claims: &MazbriClaims) {
    record_caller_rejection_scope_with_bytes(extensions, claims, None);
}

fn record_caller_rejection_scope_with_bytes(
    extensions: &axum::http::Extensions,
    claims: &MazbriClaims,
    request_bytes: Option<u64>,
) {
    let Some(slot) = extensions.get::<crate::gateway::rejection_log::RejectionSlot>() else {
        return;
    };
    slot.set(crate::gateway::rejection_log::RejectionScope {
        tenant_id: Some(claims.tenant_id.clone()),
        tenant_code: Some(claims.tenant_code.clone()),
        api_key_id: Some(claims.api_key_id.clone()).filter(|id| !id.is_empty()),
        product_id: None,
        method: None,
        path: None,
        request_bytes,
    });
}

/// Hands caller and payload to the rejection access log (no-op outside its layer).
fn record_rejection_scope(
    extensions: &axum::http::Extensions,
    claims: &MazbriClaims,
    payload: &GatewayRequestPayload,
    request_bytes: u64,
) {
    let Some(slot) = extensions.get::<crate::gateway::rejection_log::RejectionSlot>() else {
        return;
    };
    let (product_id, method, path) = match payload {
        GatewayRequestPayload::Plain(req) => (
            Some(req.product_id.clone()),
            Some(req.method.clone()),
            Some(req.path.clone()),
        ),
        GatewayRequestPayload::Encrypted(_) => (None, None, None),
    };
    slot.set(crate::gateway::rejection_log::RejectionScope {
        tenant_id: Some(claims.tenant_id.clone()),
        tenant_code: Some(claims.tenant_code.clone()),
        api_key_id: Some(claims.api_key_id.clone()).filter(|id| !id.is_empty()),
        product_id,
        method,
        path,
        request_bytes: Some(request_bytes),
    });
}

/// 按 IETF draft-ietf-httpapi-ratelimit-headers 回写租户请求配额。
fn with_ratelimit_headers(
    mut response: Response,
    quota: Option<crate::ratelimit::QuotaState>,
) -> Response {
    let Some(quota) = quota else {
        return response;
    };
    let headers = response.headers_mut();
    for (name, value) in [
        ("ratelimit-policy", quota.policy_header()),
        ("ratelimit", quota.ratelimit_header()),
    ] {
        if let Ok(value) = axum::http::HeaderValue::from_str(&value) {
            headers.insert(name, value);
        }
    }
    response
}

pub(crate) fn rate_limit_error_response(
    retry_after: i32,
    reason: &str,
    quota: Option<crate::ratelimit::QuotaState>,
) -> Response {
    let error_message = if reason == "bandwidth" {
        "带宽超过限制"
    } else {
        "请求频率超过限制"
    };
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        [(
            "retry-after",
            axum::http::HeaderValue::from_str(&retry_after.to_string())
                .unwrap_or_else(|_| axum::http::HeaderValue::from_static("60")),
        )],
        axum::Json(serde_json::json!({
            "success": false,
            "error": error_message,
            "reason": reason,
            "retry_after": retry_after,
            "status": StatusCode::TOO_MANY_REQUESTS.as_u16(),
        })),
    )
        .into_response();
    response
        .extensions_mut()
        .insert(crate::gateway::response::GatewayErrorCode(
            "E4290".to_string(),
        ));
    with_ratelimit_headers(response, quota)
}

fn current_request_id(headers: &axum::http::HeaderMap) -> String {
    parse_trace_context(headers).request_id.to_string()
}

fn parse_gateway_request_payload(body_bytes: &[u8]) -> Result<GatewayRequestPayload, JwtAuthError> {
    let body_bytes = trim_leading_ascii_whitespace(body_bytes);
    if body_bytes.starts_with(br#"{"encrypted":"#)
        && let Ok(envelope) = serde_json::from_slice::<TransportEnvelope>(body_bytes)
    {
        return Ok(GatewayRequestPayload::Encrypted(envelope));
    }

    if let Ok(proxy_req) = serde_json::from_slice::<GatewayRequest>(body_bytes) {
        return Ok(GatewayRequestPayload::Plain(proxy_req));
    }

    if contains_subslice(body_bytes, br#""encrypted""#)
        && let Ok(envelope) = serde_json::from_slice::<TransportEnvelope>(body_bytes)
    {
        return Ok(GatewayRequestPayload::Encrypted(envelope));
    }

    Err(JwtAuthError::InvalidRequestBody)
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack.windows(needle.len()).any(|w| w == needle)
}

fn trim_leading_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    &bytes[start..]
}
