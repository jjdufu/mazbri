use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::handlers::types::{CreateTenantRequest, ListParams, SetTenantStatusRequest};
use crate::middleware::auth::{AdminClaims, can_manage_tenants, extract_user_id, is_admin};
use crate::response::ApiResponse;
use crate::services::tenant as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
pub async fn list_tenants(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::list_tenants(&state.db, &params).await?;
    Ok(ApiResponse::ok(data))
}
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
pub async fn get_tenant(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::get_tenant(&state.db, &code).await?;
    Ok(ApiResponse::ok(data))
}
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
pub async fn delete_tenant(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // Hard delete: sole admin role only (not operator).
    if !is_admin(&claims) {
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
pub async fn set_tenant_status(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
    Json(payload): Json<SetTenantStatusRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let user_id = extract_user_id(&claims)?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data = svc::set_tenant_status(
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
