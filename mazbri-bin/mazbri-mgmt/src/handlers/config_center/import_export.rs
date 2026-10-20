use super::{ExportParams, ImportParams};
use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::middleware::auth::AdminClaims;
use crate::sanitize::sanitize_json_value;
use crate::services::config_center as service;
use axum::{
    Extension, Json,
    body::Body,
    extract::{ConnectInfo, Query, State},
    http::HeaderMap,
    response::Response,
};
use mazbri_core::request_context::request_ids;
use std::net::SocketAddr;
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn export_configs(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<ExportParams>,
) -> Result<Response, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let (request_id, trace_id) = request_ids(&headers);
    service::export_configs(&state, &claims, client_ip, &params, &request_id, &trace_id).await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn import_configs(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<ImportParams>,
    body: Body,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let (request_id, trace_id) = request_ids(&headers);
    Ok(Json(sanitize_json_value(
        service::import_configs(
            &state,
            &claims,
            client_ip,
            &params,
            body,
            &request_id,
            &trace_id,
        )
        .await?,
    )))
}
