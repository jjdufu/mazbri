use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::handlers::types::{CreateTenantRequest, ListParams, SetTenantEnabledRequest};
use crate::middleware::auth::{AdminClaims, can_manage_tenants, extract_user_id};
use crate::response::ApiResponse;
use crate::services::tenant as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_tenants(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::list_tenants(&state.db, &params).await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_tenant(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<CreateTenantRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let client_ip = extract_client_ip(&headers, Some(addr));
    let user_id = extract_user_id(&claims)?;
    let data = svc::create_tenant(
        &state.db,
        &state.audit,
        &payload,
        user_id,
        &claims.sub,
        client_ip,
    )
    .await?;
    Ok(crate::response::created(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_tenant(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::get_tenant(&state.db, &code).await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_tenant(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
    Json(payload): Json<svc::UpdateTenantRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let user_id = extract_user_id(&claims)?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data = svc::update_tenant(
        &state.db,
        &state.audit,
        &code,
        &payload,
        user_id,
        &claims.sub,
        client_ip,
    )
    .await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_tenant(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    _mfa: crate::handlers::mfa_guard::StepUpVerified,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // 硬删除：超级管理员与管理员（业务权限）。
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let user_id = extract_user_id(&claims)?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data = svc::delete_tenant(
        &state.db,
        &state.auth_db,
        &state.audit,
        &code,
        user_id,
        &claims.sub,
        client_ip,
    )
    .await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn set_tenant_enabled(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
    Json(payload): Json<SetTenantEnabledRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let user_id = extract_user_id(&claims)?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data = svc::set_tenant_enabled(
        &state.db,
        &state.audit,
        &code,
        &payload,
        user_id,
        &claims.sub,
        client_ip,
    )
    .await?;
    Ok(ApiResponse::ok(data))
}
