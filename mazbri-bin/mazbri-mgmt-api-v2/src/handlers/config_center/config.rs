use super::{ConfigListParams, CreateConfigRequest, UpdateConfigRequest};
use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::middleware::auth::AdminClaims;
use crate::sanitize::sanitize_json_value;
use crate::services::config_center as service;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use mazbri_core::request_context::request_ids;
use std::net::SocketAddr;
pub async fn list_configs(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<ConfigListParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let (request_id, trace_id) = request_ids(&headers);
    Ok(Json(sanitize_json_value(
        service::list_configs(&state, client_ip, &request_id, &trace_id, &params).await?,
    )))
}
pub async fn get_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(service_name): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let (request_id, trace_id) = request_ids(&headers);
    Ok(Json(sanitize_json_value(
        service::get_config(&state, &service_name, client_ip, &request_id, &trace_id).await?,
    )))
}
pub async fn create_config(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<CreateConfigRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let (request_id, trace_id) = request_ids(&headers);
    let data = service::create_config(&state, &claims, &payload, client_ip, &request_id, &trace_id)
        .await?;
    Ok((StatusCode::CREATED, Json(sanitize_json_value(data))))
}
pub async fn update_config(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(service_name): Path<String>,
    Json(payload): Json<UpdateConfigRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let (request_id, trace_id) = request_ids(&headers);
    Ok(Json(sanitize_json_value(
        service::update_config(
            &state,
            &claims,
            &service_name,
            &payload,
            client_ip,
            &request_id,
            &trace_id,
        )
        .await?,
    )))
}
pub async fn delete_config(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(service_name): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let (request_id, trace_id) = request_ids(&headers);
    Ok(Json(sanitize_json_value(
        service::delete_config(
            &state,
            &claims,
            &service_name,
            client_ip,
            &request_id,
            &trace_id,
        )
        .await?,
    )))
}
