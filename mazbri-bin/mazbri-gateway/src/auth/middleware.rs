use mazbri_core::num::SaturatingFrom;
use super::claims::MazbriClaims;
use super::jwt_keys::JwtHs256Keyring;
use crate::gateway::GatewayRequestTiming;
use crate::gateway::request_binding::{
    GatewayHttpPath, RequestBindingInput, enforce_request_binding, record_jwt_kid,
};
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
use mazbri_auth::{CredentialType, TenantContext, TokenRecorder};
use mazbri_core::crypto::transport::TransportEnvelope;
use mazbri_core::metrics::{REGISTRY, register_collector_once};
use mazbri_core::{parse_trace_context, resolve_client_ip, validate_tenant_id};
use prometheus::{IntCounterVec, Opts};
use std::net::SocketAddr;
use std::sync::LazyLock;
use std::time::Instant;
use tracing::{info, warn};
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
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_auth_middleware_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(RATELIMIT_FAIL_CLOSED_TOTAL.clone()))
}
#[derive(Debug)]
pub enum JwtAuthError {
    MissingHeader,
    InvalidToken,
    InvalidRequestBody,
    InsufficientScope,
}
impl std::fmt::Display for JwtAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JwtAuthError::InsufficientScope => write!(f, "权限不足：缺少 api:write 作用域"),
            _ => write!(f, "Token 验证失败"),
        }
    }
}
impl IntoResponse for JwtAuthError {
    fn into_response(self) -> Response {
        let status = match self {
            JwtAuthError::InvalidRequestBody => StatusCode::BAD_REQUEST,
            JwtAuthError::InsufficientScope => StatusCode::FORBIDDEN,
            _ => StatusCode::UNAUTHORIZED,
        };
        let body = serde_json::json!({
            "success": false,
            "error": self.to_string(),
            "status": status.as_u16(),
        });
        let mut response = (status, axum::Json(body)).into_response();
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
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn jwt_auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, MiddlewareRejection> {
    let started_at = Instant::now();
    let request_id = current_request_id(req.headers());
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| JwtAuthError::MissingHeader.into_response())?;
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| JwtAuthError::InvalidToken.into_response())?;
    let header = decode_header(token).map_err(|_| {
        record_jwt_kid("missing", "reject");
        JwtAuthError::InvalidToken.into_response()
    })?;
    let kid = header.kid.ok_or_else(|| {
        record_jwt_kid("missing", "reject");
        JwtAuthError::InvalidToken.into_response()
    })?;
    let decoding_key = state.auth_state.keyring.decoding_key(&kid).ok_or_else(|| {
        record_jwt_kid(&kid, "unknown");
        JwtAuthError::InvalidToken.into_response()
    })?;
    let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.set_issuer(&["mazbri.com"]);
    let claims = decode::<MazbriClaims>(token, &decoding_key, &validation)
        .map_err(|e| {
            warn!(error = % e, error_kind = ? e.kind(), kid = %kid, "JWT verification failed");
            record_jwt_kid(&kid, "reject");
            JwtAuthError::InvalidToken.into_response()
        })?
        .claims;
    record_jwt_kid(&kid, "ok");
    TokenRecorder::new("jwt").record_validated();
    let tenant_context = validate_tenant_id(&claims.tenant_id)
        .ok()
        .map(|tenant_id| TenantContext::new(tenant_id, CredentialType::Jwt));
    let (mut parts, body) = req.into_parts();
    parts.extensions.insert(claims.clone());
    parts
        .extensions
        .insert(GatewayRequestTiming::new(started_at));
    let body_bytes = body::to_bytes(body, MAX_BODY_SIZE)
        .await
        .map_err(|_| JwtAuthError::InvalidRequestBody.into_response())?;
    let path_and_query = parts
        .uri
        .path_and_query().map_or_else(|| parts.uri.path().to_string(), |value| value.as_str().to_string());
    parts
        .extensions
        .insert(GatewayHttpPath(path_and_query.clone()));
    let mut binding_outcome = crate::gateway::request_binding::RequestBindingOutcome {
        auth_mode: "bearer",
        kid: Some(kid.clone()),
        nonce_trunc: None,
        sig_ok: None,
    };
    if parts.uri.path() == "/api/v1/gateway" {
        if !claims.can_invoke() {
            warn!(
                tenant_id = %claims.tenant_id,
                scopes = ?claims.scopes,
                "Gateway invoke denied: JWT missing api:write scope"
            );
            return Err(JwtAuthError::InsufficientScope.into_response().into());
        }
        if let Some(ctx) = tenant_context.clone() {
            let payload =
                parse_gateway_request_payload(&body_bytes).map_err(IntoResponse::into_response)?;
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
        }
        if let Some(tenant_id) = tenant_context.as_ref().map(|ctx| ctx.tenant_id) {
            binding_outcome = enforce_request_binding(RequestBindingInput {
                crypto: &state.proxy_state.crypto_state,
                tenant_id,
                require: state.request_binding.require_request_binding,
                nonce_prefix: &state.request_binding.nonce_redis_prefix,
                method: "POST",
                path_and_query: &path_and_query,
                headers: &parts.headers,
                body: &body_bytes,
            })
            .await
            .map_err(IntoResponse::into_response)?;
            if binding_outcome.kid.is_none() {
                binding_outcome.kid = Some(kid.clone());
            }
        }
    } else if let Some(ctx) = tenant_context {
        parts.extensions.insert(ctx);
    }
    if let Some(timing) = parts.extensions.get_mut::<GatewayRequestTiming>() {
        timing.auth_ms = mazbri_core::num::millis_u64(started_at.elapsed());
    }
    let auth_stage_ms = mazbri_core::num::millis_u64(started_at.elapsed());
    if crate::gateway::stage_log::should_log_stage_info(auth_stage_ms) {
        info!(
            target : "gateway.stage", service = "mazbri-gateway", stage_name = "auth",
            stage_ms = auth_stage_ms, request_id = % request_id,
            tenant_id = % claims.tenant_id, tenant_code = % claims.tenant_code,
            auth_mode = binding_outcome.auth_mode,
            kid = binding_outcome.kid.as_deref().unwrap_or(""),
            nonce = binding_outcome.nonce_trunc.as_deref().unwrap_or(""),
            sig_ok = binding_outcome.sig_ok,
            encrypted_request = matches!(parts.extensions.get::<GatewayRequestPayload>(), Some(GatewayRequestPayload::Encrypted(_))),
            message = "Gateway auth stage completed"
        );
    }
    Ok(next
        .run(Request::from_parts(parts, Body::from(body_bytes)))
        .await)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn ratelimit_middleware(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    mut req: Request,
    next: Next,
) -> Result<Response, MiddlewareRejection> {
    let tenant_context = req
        .extensions()
        .get::<TenantContext>()
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!(
                    { "success" : false, "error" : "未找到租户上下文",
                    "status" : StatusCode::UNAUTHORIZED.as_u16(), }
                )),
            )
                .into_response()
        })?;
    let api_endpoint = req
        .extensions()
        .get::<ProxyExecutionContext>().map_or_else(|| req.uri().path().to_string(), |ctx| ctx.api_endpoint.clone());
    let client_ip = resolve_client_ip(req.headers(), Some(peer_addr));
    let request_id = current_request_id(req.headers());
    let request_bytes = content_length_header(req.headers());
    let stage_start = Instant::now();
    let mut timing = req
        .extensions()
        .get::<GatewayRequestTiming>()
        .cloned()
        .unwrap_or_else(|| GatewayRequestTiming::new(stage_start));
    let decision = state
        .proxy_state
        .local_ratelimiter
        .check(&LocalRateLimitInput {
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
        } => {
            timing.bandwidth_prepaid = Some(BandwidthPrepaid {
                policy_version,
                snapshot_age_ms,
                degraded,
                prepaid_request_bytes,
                bandwidth_enabled,
            });
            req.extensions_mut().insert(timing);
            let rl_ms = mazbri_core::num::millis_u64(stage_start.elapsed());
            if crate::gateway::stage_log::should_log_stage_info(rl_ms) {
                info!(
                    target : "gateway.stage", service = "mazbri-gateway", stage_name =
                    "ratelimit", stage_ms = rl_ms, tenant_id
                    = % tenant_context.tenant_id, api_endpoint = % api_endpoint, request_id =
                    % request_id, policy_version, snapshot_age_ms, degraded, source = "local",
                    decision = "allow", prepaid_request_bytes, bandwidth_enabled,
                    message = "Gateway rate limit stage completed"
                );
            }
            Ok(next.run(req).await)
        }
        LocalRateLimitDecision::Deny {
            policy_version,
            retry_after_ms,
            reason,
            snapshot_age_ms,
            degraded,
        } => {
            warn!(
                target : "gateway.stage", service = "mazbri-gateway", stage_name =
                "ratelimit", stage_ms = mazbri_core::num::millis_u64(stage_start.elapsed()), tenant_id
                = % tenant_context.tenant_id, api_endpoint = % api_endpoint, request_id =
                % request_id, policy_version, retry_after_ms, reason = % reason,
                snapshot_age_ms, degraded, source = "local", decision = "deny",
                message = "Gateway rate limit stage rejected request"
            );
            Err(rate_limit_error_response(i32::saturating_from((retry_after_ms / 1000).max(1)), &reason).into())
        }
    }
}

fn content_length_header(headers: &axum::http::HeaderMap) -> u64 {
    headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

fn rate_limit_error_response(retry_after: i32, reason: &str) -> Response {
    let error_message = if reason == "bandwidth" {
        "带宽超过限制"
    } else {
        "请求频率超过限制"
    };
    (
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
        .into_response()
}
fn current_request_id(headers: &axum::http::HeaderMap) -> String {
    parse_trace_context(headers).request_id.to_string()
}

fn parse_gateway_request_payload(body_bytes: &[u8]) -> Result<GatewayRequestPayload, JwtAuthError> {
    let body_bytes = trim_leading_ascii_whitespace(body_bytes);
    if body_bytes.starts_with(
        br#"{"encrypted":#) {
        if let Ok(envelope) = serde_json::from_slice::<TransportEnvelope>(body_bytes) {
            return Ok(GatewayRequestPayload::Encrypted(envelope));
        }
    } else if body_bytes.starts_with(br#"{"product_id":"#,
    ) {
        if let Ok(proxy_req) = serde_json::from_slice::<GatewayRequest>(body_bytes) {
            return Ok(GatewayRequestPayload::Plain(proxy_req));
        }
    }

    if let Ok(envelope) = serde_json::from_slice::<TransportEnvelope>(body_bytes) {
        return Ok(GatewayRequestPayload::Encrypted(envelope));
    }

    let proxy_req = serde_json::from_slice::<GatewayRequest>(body_bytes)
        .map_err(|_| JwtAuthError::InvalidRequestBody)?;
    Ok(GatewayRequestPayload::Plain(proxy_req))
}

fn trim_leading_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    &bytes[start..]
}
