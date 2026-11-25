use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::handlers::types::ListParams;
use crate::middleware::auth::{AdminClaims, extract_user_id, is_admin, is_business_admin};
use crate::response::ApiResponse;
use crate::services::route_management as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use sqlx::Row;

/// 平台全局路由版本（供 Web 与网关 admin status 对比落后实例）。
/// GET /api/v1/mgmt/gateway/route-version
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_gateway_route_version(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let row = sqlx::query(
        "SELECT version, updated_at FROM route_sync_metadata WHERE key = 'global_route_version'",
    )
    .fetch_one(state.db.as_ref())
    .await
    .map_err(|e| ApiError::internal(format!("读取路由版本失败: {e}")))?;
    let version: i64 = row
        .try_get("version")
        .map_err(|e| ApiError::internal(format!("读取 version 失败: {e}")))?;
    let updated_at: chrono::DateTime<chrono::Utc> = row
        .try_get("updated_at")
        .map_err(|e| ApiError::internal(format!("读取 updated_at 失败: {e}")))?;
    Ok(ApiResponse::ok(serde_json::json!({
        "version": version,
        "updated_at": updated_at,
    })))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_channels(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Query(params): Query<ListParams>,
) -> Result<impl IntoResponse, ApiError> {
    let mut data = svc::list_channels(&state.db, &params).await?;
    crate::middleware::role_projection::RoleDataProjection::for_claims(&claims)
        .project_channel_value(&mut data);
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_channel(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<svc::CreateChannelRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_business_admin(&claims) {
        return Err(ApiError::Forbidden);
    }
    crate::services::upstream_connector::reject_client_enc_in_channel_config(&payload.config)?;
    if payload
        .config
        .get("egress")
        .and_then(|e| e.get("tls"))
        .is_some_and(|t| !t.is_null())
    {
        crate::services::root_package::ensure_root_material_keyring_for_write_raw(
            &state.cert_http_client,
            &state.cert_service.url,
            &state.cert_service.token,
        )
        .await?;
    }
    let data = svc::create_channel(
        &state.db,
        state.connector_runtime.ssrf(),
        &state.audit,
        &payload,
        extract_user_id(&claims)?,
        is_admin(&claims),
        extract_client_ip(&headers, Some(addr)),
    )
    .await?;
    Ok(crate::response::created(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_channel(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let mut data = svc::get_channel(&state.db, &id).await?;
    crate::middleware::role_projection::RoleDataProjection::for_claims(&claims)
        .project_channel_value(&mut data);
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_channel(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(payload): Json<svc::UpdateChannelRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_business_admin(&claims) {
        return Err(ApiError::Forbidden);
    }
    if let Some(ref raw_cfg) = payload.config {
        crate::services::upstream_connector::reject_client_enc_in_channel_config(raw_cfg)?;
        if raw_cfg
            .get("egress")
            .and_then(|e| e.get("tls"))
            .is_some_and(|t| !t.is_null())
        {
            crate::services::root_package::ensure_root_material_keyring_for_write_raw(
                &state.cert_http_client,
                &state.cert_service.url,
                &state.cert_service.token,
            )
            .await?;
        }
    }
    let data = svc::update_channel(svc::UpdateChannelArgs {
        pool: &state.db,
        ssrf_checker: state.connector_runtime.ssrf(),
        audit: &state.audit,
        id: &id,
        payload: &payload,
        user_id: extract_user_id(&claims)?,
        is_super_admin: is_admin(&claims),
        client_ip: extract_client_ip(&headers, Some(addr)),
    })
    .await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_channel(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_business_admin(&claims) {
        return Err(ApiError::Forbidden);
    }
    let data = svc::delete_channel(
        &state.db,
        &state.audit,
        &id,
        extract_user_id(&claims)?,
        extract_client_ip(&headers, Some(addr)),
    )
    .await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_route_rules(
    State(state): State<AppState>,
    Query(params): Query<svc::RouteRuleListParams>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::list_route_rules(&state.db, &params).await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_route_rule(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<svc::CreateRouteRuleRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_business_admin(&claims) {
        return Err(ApiError::Forbidden);
    }
    let data = svc::create_route_rule(
        &state.db,
        &state.audit,
        &payload,
        extract_user_id(&claims)?,
        extract_client_ip(&headers, Some(addr)),
    )
    .await?;
    Ok(crate::response::created(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_route_rule(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::get_route_rule(&state.db, &id).await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_route_rule(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(payload): Json<svc::UpdateRouteRuleRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_business_admin(&claims) {
        return Err(ApiError::Forbidden);
    }
    let data = svc::update_route_rule(
        &state.db,
        &state.audit,
        &id,
        &payload,
        extract_user_id(&claims)?,
        extract_client_ip(&headers, Some(addr)),
    )
    .await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_route_rule(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_business_admin(&claims) {
        return Err(ApiError::Forbidden);
    }
    let data = svc::delete_route_rule(
        &state.db,
        &state.audit,
        &id,
        extract_user_id(&claims)?,
        extract_client_ip(&headers, Some(addr)),
    )
    .await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn emergency_switch_route(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(payload): Json<svc::EmergencySwitchRequest>,
) -> Result<Response, ApiError> {
    if !is_business_admin(&claims) {
        return Err(ApiError::Forbidden);
    }
    let data = svc::emergency_switch_route(
        &state.db,
        &state.audit,
        &id,
        &payload,
        extract_user_id(&claims)?,
        &claims.sub,
        extract_client_ip(&headers, Some(addr)),
    )
    .await?;
    Ok(ApiResponse::ok(data).into_response())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_route_bindings(
    State(state): State<AppState>,
    Path(route_rule_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::list_route_bindings(&state.db, &route_rule_id).await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_route_binding(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(route_rule_id): Path<String>,
    Json(payload): Json<svc::CreateRouteBindingRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_business_admin(&claims) {
        return Err(ApiError::Forbidden);
    }
    let data = svc::create_route_binding(
        &state.db,
        &state.audit,
        &route_rule_id,
        &payload,
        extract_user_id(&claims)?,
        extract_client_ip(&headers, Some(addr)),
    )
    .await?;
    Ok(crate::response::created(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_route_binding(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(payload): Json<svc::UpdateRouteBindingRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_business_admin(&claims) {
        return Err(ApiError::Forbidden);
    }
    let data = svc::update_route_binding(
        &state.db,
        &state.audit,
        &id,
        &payload,
        extract_user_id(&claims)?,
        extract_client_ip(&headers, Some(addr)),
    )
    .await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_route_binding(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_business_admin(&claims) {
        return Err(ApiError::Forbidden);
    }
    let data = svc::delete_route_binding(
        &state.db,
        &state.audit,
        &id,
        extract_user_id(&claims)?,
        extract_client_ip(&headers, Some(addr)),
    )
    .await?;
    Ok(ApiResponse::ok(data))
}
