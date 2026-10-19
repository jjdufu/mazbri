use super::NodeListParams;
use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::sanitize::sanitize_json_value;
use crate::services::config_center as service;
use axum::{
    Json,
    extract::{ConnectInfo, Query, State},
    http::HeaderMap,
};
use mazbri_core::request_context::request_ids;
use std::net::SocketAddr;
pub async fn list_nodes(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<NodeListParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let (request_id, trace_id) = request_ids(&headers);
    Ok(Json(sanitize_json_value(
        service::list_nodes(&state, &params, client_ip, &request_id, &trace_id).await?,
    )))
}
