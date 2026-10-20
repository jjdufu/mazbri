use super::{cert_service_request, dependency_internal_error};
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::handlers::config_center::{NodeListParams, ServiceListParams};
const MAX_SERVICE_FILTER_CONFIGS: usize = 1000;
pub async fn list_services(
    state: &AppState,
    _params: &ServiceListParams,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    let url_path = format!("/configs/search?limit={MAX_SERVICE_FILTER_CONFIGS}&offset=0");
    let resp = cert_service_request(
        state,
        reqwest::Method::GET,
        &url_path,
        None,
        client_ip,
        request_id,
        trace_id,
    )
    .await?;
    let configs: Vec<serde_json::Value> = resp.json().await.map_err(|e| {
        dependency_internal_error(
            "config_center_list_services_failed",
            request_id,
            trace_id,
            &e,
        )
    })?;
    let mut services = configs
        .into_iter()
        .filter_map(|config| {
            config
                .get("service_name")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    services.sort();
    services.dedup();
    Ok(serde_json::json!({ "services" : services }))
}
pub async fn list_nodes(
    state: &AppState,
    _params: &NodeListParams,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    let resp = cert_service_request(
        state,
        reqwest::Method::GET,
        "/nodes",
        None,
        client_ip,
        request_id,
        trace_id,
    )
    .await?;
    let node_list: Vec<serde_json::Value> = resp.json().await.map_err(|e| {
        dependency_internal_error("config_center_list_nodes_failed", request_id, trace_id, &e)
    })?;
    Ok(serde_json::json!({ "nodes" : node_list, "total" : node_list.len() }))
}
