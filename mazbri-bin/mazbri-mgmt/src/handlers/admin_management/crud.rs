use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::state::AppState;
use crate::handlers::types::{AdminListParams, CreateAdminRequest, UpdateAdminRequest};
use crate::middleware::auth::AdminClaims;
use crate::response::ApiResponse;
use crate::services::admin_management as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    response::IntoResponse,
};
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_admins(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Query(params): Query<AdminListParams>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::list_admins(&state.admin_db, &claims, &params).await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_admin(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateAdminRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data = svc::create_admin(
        &state.admin_db,
        &state.audit,
        &state.password_policy_cache,
        &claims,
        &req,
        client_ip,
    )
    .await?;
    Ok(crate::response::created(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_admin(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::get_admin(&state.admin_db, &claims, id).await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_admin(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<UpdateAdminRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data =
        svc::update_admin(&state.admin_db, &state.audit, &claims, id, &req, client_ip).await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_admin(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data = svc::delete_admin(&state.admin_db, &state.audit, &claims, id, client_ip).await?;
    Ok(ApiResponse::ok(data))
}
