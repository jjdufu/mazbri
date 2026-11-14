use super::ServiceListParams;
use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::middleware::auth::AdminClaims;
use crate::services::config_center as service;
use axum::{
    Extension, Json,
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
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<ServiceListParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    super::require_config_admin(&claims)?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    let (request_id, trace_id) = request_ids(&headers);
    Ok(Json(
        service::list_services(&state, &params, client_ip, &request_id, &trace_id).await?,
    ))
}

fn canonical_service_names_value() -> serde_json::Value {
    serde_json::json!({
        "services": mazbri_core::ServiceName::all_names(),
        "mtls_services": mazbri_core::ServiceName::mtls_names(),
        "global": mazbri_core::GLOBAL_CONFIG_NAME,
    })
}

/// 规范服务名单一真源：前端选择器与配置中心均以此为准（仅超级管理员可访问）。
///
/// # Errors
///
/// 非超级管理员访问时返回 `ApiError::Forbidden`。
pub async fn list_canonical_service_names(
    Extension(claims): Extension<AdminClaims>,
) -> Result<Json<serde_json::Value>, ApiError> {
    super::require_config_admin(&claims)?;
    Ok(Json(canonical_service_names_value()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_core::{CANONICAL_SERVICES, GLOBAL_CONFIG_NAME};

    #[test]
    fn canonical_names_match_core_order() {
        let body = canonical_service_names_value();
        let names: Vec<&str> = body["services"]
            .as_array()
            .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default();
        let expected: Vec<&str> = CANONICAL_SERVICES.iter().map(|s| s.as_str()).collect();
        assert_eq!(names.len(), 11);
        assert_eq!(names, expected);
        assert_eq!(body["global"], GLOBAL_CONFIG_NAME);
        let mtls: Vec<&str> = body["mtls_services"]
            .as_array()
            .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default();
        assert_eq!(mtls.len(), 8);
        for excluded in ["mazbri-cert", "mazbri-mgmt", "mazbri-analytics"] {
            assert!(!mtls.contains(&excluded), "{excluded}");
        }
    }
}
