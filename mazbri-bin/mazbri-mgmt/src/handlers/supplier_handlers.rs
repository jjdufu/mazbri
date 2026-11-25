use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::handlers::types::ListParams;
use crate::middleware::auth::{AdminClaims, extract_user_id, is_admin, is_business_admin};
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
    if let Some(ref raw_cfg) = payload.config {
        crate::services::upstream_connector::reject_client_enc_in_supplier_config(raw_cfg)?;
        if crate::services::upstream_connector::supplier_config_has_secrets_to_seal(raw_cfg) {
            crate::services::root_package::ensure_root_material_keyring_for_write_raw(
                &state.cert_http_client,
                &state.cert_service.url,
                &state.cert_service.token,
            )
            .await?;
        }
    }
    let client_ip = extract_client_ip(&headers, Some(addr));
    let user_id = extract_user_id(&claims)?;
    let data = svc::create_supplier(
        &state.db,
        state.connector_runtime.ssrf(),
        &state.audit,
        &payload,
        svc::SupplierActorCtx {
            user_id,
            username: &claims.sub,
            is_super_admin: is_admin(&claims),
            client_ip,
        },
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
    if let Some(ref raw_cfg) = payload.config {
        crate::services::upstream_connector::reject_client_enc_in_supplier_config(raw_cfg)?;
        if crate::services::upstream_connector::supplier_config_has_secrets_to_seal(raw_cfg) {
            crate::services::root_package::ensure_root_material_keyring_for_write_raw(
                &state.cert_http_client,
                &state.cert_service.url,
                &state.cert_service.token,
            )
            .await?;
        }
    }
    let user_id = extract_user_id(&claims)?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data = svc::update_supplier(
        &state.db,
        state.connector_runtime.ssrf(),
        &state.audit,
        &id,
        &payload,
        svc::SupplierActorCtx {
            user_id,
            username: &claims.sub,
            is_super_admin: is_admin(&claims),
            client_ip,
        },
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
