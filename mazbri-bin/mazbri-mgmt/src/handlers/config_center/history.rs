use super::HistoryQueryParams;
use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::middleware::auth::AdminClaims;
use crate::services::config_center as service;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
};
use mazbri_core::request_context::request_ids;
use std::net::SocketAddr;
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(service_name): Path<String>,
    Query(params): Query<HistoryQueryParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let (request_id, trace_id) = request_ids(&headers);
    Ok(Json(
        service::get_history(
            &state,
            &service_name,
            &params,
            client_ip,
            &request_id,
            &trace_id,
        )
        .await?,
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn rollback(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path((service_name, version)): Path<(String, i32)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let (request_id, trace_id) = request_ids(&headers);
    Ok(Json(
        service::rollback(
            &state,
            &claims,
            &service_name,
            version,
            client_ip,
            &request_id,
            &trace_id,
        )
        .await?,
    ))
}
