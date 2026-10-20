use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::middleware::auth::{AdminClaims, extract_user_id, is_operator_or_above};
use crate::response::ApiResponse;
use crate::services::callback as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_subscriptions(
    State(state): State<AppState>,
    Query(query): Query<svc::CallbackListQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::list_subscriptions(
        &state.db,
        query.q.as_deref(),
        query.filters.as_deref(),
        query.page,
        query.page_size,
    )
    .await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_subscription(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<svc::CreateSubscriptionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_operator_or_above(&claims) {
        return Err(ApiError::Forbidden);
    }
    let client_ip = extract_client_ip(&headers, Some(addr));
    let user_id = extract_user_id(&claims)?;
    let data = svc::create_subscription(
        &state.db,
        &state.audit,
        payload,
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
pub async fn delete_subscription(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_operator_or_above(&claims) {
        return Err(ApiError::Forbidden);
    }
    let user_id = extract_user_id(&claims)?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    svc::delete_subscription(&state.db, &state.audit, id, user_id, &claims.sub, client_ip).await?;
    Ok(ApiResponse::ok(
        serde_json::json!({ "message" : "Subscription deleted" }),
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_delivery_logs(
    State(state): State<AppState>,
    Query(query): Query<svc::CallbackListQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::list_delivery_logs(
        &state.db,
        query.q.as_deref(),
        query.filters.as_deref(),
        query.page,
        query.page_size,
    )
    .await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn retry_delivery(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_operator_or_above(&claims) {
        return Err(ApiError::Forbidden);
    }
    let user_id = extract_user_id(&claims)?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    svc::retry_delivery(&state.db, id).await?;
    state.audit.log(create_audit_log(
        user_id,
        AuditAction::CallbackDeliveryRetry,
        AuditResourceType::Callback,
        id.to_string(),
        Some(serde_json::json!({ "delivery_log_id" : id })),
        client_ip,
    ));
    Ok(ApiResponse::ok(
        serde_json::json!({ "message" : "Delivery retry scheduled" }),
    ))
}
