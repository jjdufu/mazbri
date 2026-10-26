use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::handlers::types::ListParams;
use crate::middleware::auth::{AdminClaims, extract_user_id, is_business_admin};
use crate::response::ApiResponse;
use crate::services::supplier as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_suppliers(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::list_suppliers(&state.db, &params).await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_supplier(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<svc::CreateSupplierRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_business_admin(&claims) {
        return Err(ApiError::Forbidden);
    }
    let client_ip = extract_client_ip(&headers, Some(addr));
    let user_id = extract_user_id(&claims)?;
    let data = svc::create_supplier(
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
pub async fn get_supplier(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::get_supplier(&state.db, &id).await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_supplier(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(payload): Json<svc::UpdateSupplierRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_business_admin(&claims) {
        return Err(ApiError::Forbidden);
    }
    let user_id = extract_user_id(&claims)?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data = svc::update_supplier(
        &state.db,
        &state.audit,
        &id,
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
pub async fn delete_supplier(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_business_admin(&claims) {
        return Err(ApiError::Forbidden);
    }
    let user_id = extract_user_id(&claims)?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data = svc::delete_supplier(
        &state.db,
        &state.audit,
        &id,
        user_id,
        &claims.sub,
        client_ip,
    )
    .await?;
    Ok(ApiResponse::ok(data))
}
