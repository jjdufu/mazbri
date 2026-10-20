use super::ServiceListParams;
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
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_services(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<ServiceListParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let (request_id, trace_id) = request_ids(&headers);
    Ok(Json(sanitize_json_value(
        service::list_services(&state, &params, client_ip, &request_id, &trace_id).await?,
    )))
}

/// 规范服务名单一真源：前端选择器与配置中心均以此为准。
pub async fn list_canonical_service_names() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "services": mazbri_core::ServiceName::all_names(),
        "global": mazbri_core::GLOBAL_CONFIG_NAME,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_core::{CANONICAL_SERVICES, GLOBAL_CONFIG_NAME};

    #[tokio::test]
    async fn canonical_names_match_core_order() {
        let Json(body) = list_canonical_service_names().await;
        let names: Vec<&str> = body["services"]
            .as_array()
            .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default();
        let expected: Vec<&str> = CANONICAL_SERVICES.iter().map(|s| s.as_str()).collect();
        assert_eq!(names.len(), 11);
        assert_eq!(names, expected);
        assert_eq!(body["global"], GLOBAL_CONFIG_NAME);
    }
}
