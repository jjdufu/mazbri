use super::AppState;
use super::types::{CreateApiKeyRequest, UpdateApiKeyRequest};

use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::middleware::ClientIp;
use crate::middleware::auth::{AdminClaims, can_manage_tenants, extract_user_id};
use crate::response::{ApiResponse, created};
use crate::services::api_key as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, State},
    http::HeaderMap,
    response::IntoResponse,
};
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_api_key(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Extension(ClientIp(client_ip)): Extension<ClientIp>,
    Path(code): Path<String>,
    Json(payload): Json<CreateApiKeyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = extract_user_id(&claims)?;
    let data = svc::create_api_key(
        &state.db,
        &state.auth_db,
        &state.audit,
        &code,
        &payload,
        user_id,
        Some(client_ip),
    )
    .await?;
    Ok(created(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_api_keys(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::list_api_keys(&state.db, &state.auth_db, &code).await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn revoke_api_key(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path((code, key_prefix)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = extract_user_id(&claims)?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data = svc::revoke_api_key(svc::RevokeApiKeyParams {
        db: &state.db,
        auth_db: &state.auth_db,
        audit: &state.audit,
        id_or_code: &code,
        key_prefix: &key_prefix,
        user_id,
        username: &claims.sub,
        client_ip,
    })
    .await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_api_key(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path((code, key_prefix)): Path<(String, String)>,
    Json(payload): Json<UpdateApiKeyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // 续期 API 密钥属于租户写操作，与租户管理权限一致。
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    payload.validate().map_err(ApiError::validation)?;
    let data = svc::update_api_key(&state.db, &state.auth_db, &code, &key_prefix, &payload).await?;
    Ok(ApiResponse::ok(data))
}
