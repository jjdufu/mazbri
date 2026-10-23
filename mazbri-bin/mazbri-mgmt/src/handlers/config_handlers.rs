use super::AppState;
use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::middleware::auth::{AdminClaims, can_manage_tenants};
use crate::response::ApiResponse;
use crate::services::config as svc;
use axum::http::StatusCode;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, State},
    http::HeaderMap,
    response::IntoResponse,
};
pub async fn get_ratelimit_config(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> axum::response::Response {
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    match svc::get_ratelimit_config(&state.db, &state.ratelimit_db, tenant_id).await {
        Ok(data) => ApiResponse::ok(data).into_response(),
        Err(e) if e.starts_with("Tenant not found") => ApiResponse::err(StatusCode::NOT_FOUND, e),
        Err(e) => ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}
pub async fn update_ratelimit_config(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
    Json(payload): Json<svc::RateLimitConfigUpdateRequest>,
) -> axum::response::Response {
    if !can_manage_tenants(&claims) {
        return ApiResponse::err(StatusCode::FORBIDDEN, "Insufficient permissions");
    }
    let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) else {
        return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
    };
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    match svc::update_ratelimit_config(&state.db, &state.ratelimit_db, tenant_id, &payload).await {
        Ok(data) => {
            let client_ip = extract_client_ip(&headers, Some(addr));
            state.audit.log(create_audit_log(
                operator_id,
                AuditAction::TenantRateLimitUpdate,
                AuditResourceType::Tenant,
                tenant_id.to_string(),
                Some(serde_json::json!({
                    "tenant_code": code,
                    "tenant_id": tenant_id,
                    "max_concurrent": payload.max_concurrent,
                    "max_bandwidth_mbps": payload.max_bandwidth_mbps,
                    "api_limits": &payload.api_limits,
                    "burst_size": payload.burst_size,
                    "client_ip_qps": payload.client_ip_qps,
                    "client_ip_burst": payload.client_ip_burst,
                    "enabled": payload.enabled,
                    "expected_updated_at": payload.expected_updated_at,
                })),
                client_ip,
            ));
            ApiResponse::ok(data).into_response()
        }
        Err(svc::ConfigUpdateError::TenantNotFound(e)) => {
            ApiResponse::err(StatusCode::NOT_FOUND, e)
        }
        Err(svc::ConfigUpdateError::Validation(e)) => ApiResponse::err(StatusCode::BAD_REQUEST, e),
        Err(svc::ConfigUpdateError::Conflict(e)) => ApiResponse::err(StatusCode::CONFLICT, e),
        Err(svc::ConfigUpdateError::Internal(e)) => {
            ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, e)
        }
    }
}
pub async fn get_global_ratelimit_settings(
    State(state): State<AppState>,
) -> axum::response::Response {
    match svc::get_global_ratelimit_settings(&state.db, &state.ratelimit_db).await {
        Ok(data) => ApiResponse::ok(data).into_response(),
        Err(e) => ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}
pub async fn delete_ratelimit_config(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> axum::response::Response {
    if !can_manage_tenants(&claims) {
        return ApiResponse::err(StatusCode::FORBIDDEN, "Insufficient permissions");
    }
    let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) else {
        return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
    };
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    match svc::delete_ratelimit_config(&state.ratelimit_db, tenant_id).await {
        Ok(true) => {
            let client_ip = extract_client_ip(&headers, Some(addr));
            state.audit.log(create_audit_log(
                operator_id,
                AuditAction::TenantRateLimitDelete,
                AuditResourceType::Tenant,
                tenant_id.to_string(),
                Some(serde_json::json!({ "tenant_code" : code, "tenant_id" : tenant_id })),
                client_ip,
            ));
            ApiResponse::ok(serde_json::json!({ "message" : "Rate limit configuration deleted" }))
                .into_response()
        }
        Ok(false) => ApiResponse::err(
            StatusCode::NOT_FOUND,
            "No rate limit configuration found for this tenant",
        ),
        Err(e) => ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}
pub async fn get_cache_config(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> axum::response::Response {
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    match svc::get_cache_config(&state.db, &state.cache_db, tenant_id).await {
        Ok(data) => ApiResponse::ok(data).into_response(),
        Err(e) if e.starts_with("Tenant not found") => ApiResponse::err(StatusCode::NOT_FOUND, e),
        Err(e) => ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}
pub async fn update_cache_config(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
    Json(payload): Json<svc::UpdateCacheConfigRequest>,
) -> axum::response::Response {
    if !can_manage_tenants(&claims) {
        return ApiResponse::err(StatusCode::FORBIDDEN, "Insufficient permissions");
    }
    let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) else {
        return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
    };
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    match svc::update_cache_config(&state.db, &state.cache_db, tenant_id, &payload).await {
        Ok(data) => {
            let client_ip = extract_client_ip(&headers, Some(addr));
            state.audit.log(create_audit_log(
                operator_id,
                AuditAction::TenantCacheUpdate,
                AuditResourceType::Tenant,
                tenant_id.to_string(),
                Some(serde_json::json!(
                    { "tenant_code" : code, "tenant_id" : tenant_id, "enabled" :
                    payload.enabled, "default_ttl" : payload.default_ttl,
                    "max_cache_size" : payload.max_cache_size, "api_ttl" :
                    &payload.api_ttl, "tenant_isolated" : payload
                    .tenant_isolated, "warmup_enabled" : payload
                    .warmup_enabled, "warmup_keys" : &payload.warmup_keys,
                    "expected_updated_at" : payload.expected_updated_at }
                )),
                client_ip,
            ));
            ApiResponse::ok(data).into_response()
        }
        Err(svc::ConfigUpdateError::TenantNotFound(e)) => {
            ApiResponse::err(StatusCode::NOT_FOUND, e)
        }
        Err(svc::ConfigUpdateError::Validation(e)) => ApiResponse::err(StatusCode::BAD_REQUEST, e),
        Err(svc::ConfigUpdateError::Conflict(e)) => ApiResponse::err(StatusCode::CONFLICT, e),
        Err(svc::ConfigUpdateError::Internal(e)) => {
            ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, e)
        }
    }
}
