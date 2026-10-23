use super::{cert_service_request, dependency_internal_error, not_found_error};
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::handlers::config_center::{ConfigListParams, MAX_PAGE_SIZE};
use std::fmt::Write as _;
pub async fn list_configs(
    state: &AppState,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
    params: &ConfigListParams,
) -> Result<serde_json::Value, ApiError> {
    let page_size = params.page_size.min(MAX_PAGE_SIZE);
    let offset = (params.page.saturating_sub(1)) * page_size;
    let filters = crate::list_query::parse_and_validate_filters(
        params.filters.as_deref(),
        &crate::list_query::config_center_catalog(),
    )
    .map_err(|e| ApiError::validation(e.message))?;

    // Map structured filters onto cert-service search query params (server-side).
    let mut service_name = params.service_name.clone();
    let mut updated_by: Option<String> = None;
    let mut updated_after: Option<String> = None;
    let mut updated_before: Option<String> = None;
    for f in &filters {
        match (f.field.as_str(), f.op, f.value.as_ref()) {
            ("service_name", crate::list_query::FilterOp::Contains, Some(v)) => {
                service_name = Some(filter_value_as_string(v));
            }
            ("service_name", crate::list_query::FilterOp::Eq, Some(v)) => {
                // Exact: pass without wrapping; cert still ILIKE — use exact by
                // relying on full string match when user chooses eq.
                service_name = Some(filter_value_as_string(v));
            }
            (
                "updated_by",
                crate::list_query::FilterOp::Contains | crate::list_query::FilterOp::Eq,
                Some(v),
            ) => {
                updated_by = Some(filter_value_as_string(v));
            }
            ("updated_at", crate::list_query::FilterOp::Gte, Some(v)) => {
                updated_after = Some(filter_value_as_string(v));
            }
            ("updated_at", crate::list_query::FilterOp::Lte, Some(v)) => {
                updated_before = Some(filter_value_as_string(v));
            }
            ("updated_at", crate::list_query::FilterOp::Between, Some(v)) => {
                if let Some((a, b)) = filter_value_as_pair(v) {
                    updated_after = Some(a);
                    updated_before = Some(b);
                }
            }
            _ => {
                return Err(ApiError::validation(format!(
                    "config_center filter not supported by proxy: {} {}",
                    f.field,
                    f.op.as_str()
                )));
            }
        }
    }

    let mut url_path = format!("/configs/search?limit={page_size}&offset={offset}");
    if let Some(ref sn) = service_name {
        let _ = write!(url_path, "&service_name={}", urlencoding::encode(sn));
    }
    if let Some(ref search) = params.q
        && service_name.is_none()
    {
        let _ = write!(url_path, "&q={}", urlencoding::encode(search));
    }
    if let Some(ref ub) = updated_by {
        let _ = write!(url_path, "&updated_by={}", urlencoding::encode(ub));
    }
    if let Some(ref gte) = updated_after {
        let _ = write!(url_path, "&updated_after={}", urlencoding::encode(gte));
    }
    if let Some(ref lte) = updated_before {
        let _ = write!(url_path, "&updated_before={}", urlencoding::encode(lte));
    }
    let resp = cert_service_request(
        state,
        reqwest::Method::GET,
        &url_path,
        None,
        client_ip.clone(),
        request_id,
        trace_id,
    )
    .await?;
    let configs: Vec<serde_json::Value> = resp.json().await.map_err(|e| {
        dependency_internal_error("config_center_list_failed", request_id, trace_id, &e)
    })?;
    let count = configs.len();
    let total = if count < page_size as usize {
        offset as usize + count
    } else {
        (offset as usize + count) + 1
    };
    Ok(serde_json::json!(
        { "configs" : configs, "total" : total, "page" : params.page, "page_size" :
        page_size }
    ))
}

fn filter_value_as_string(v: &crate::list_query::FilterValue) -> String {
    match v {
        crate::list_query::FilterValue::String(s) => s.clone(),
        crate::list_query::FilterValue::Number(n) => n.to_string(),
        crate::list_query::FilterValue::Bool(b) => b.to_string(),
        crate::list_query::FilterValue::StringArray(a) => a.join(","),
        crate::list_query::FilterValue::NumberArray(a) => a
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(","),
        crate::list_query::FilterValue::Null => String::new(),
    }
}

fn filter_value_as_pair(v: &crate::list_query::FilterValue) -> Option<(String, String)> {
    match v {
        crate::list_query::FilterValue::StringArray(a) if a.len() == 2 => {
            Some((a[0].clone(), a[1].clone()))
        }
        _ => None,
    }
}
pub async fn get_config(
    state: &AppState,
    service_name: &str,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    let url_path = format!("/configs/{}", urlencoding::encode(service_name));
    let resp = cert_service_request(
        state,
        reqwest::Method::GET,
        &url_path,
        None,
        client_ip.clone(),
        request_id,
        trace_id,
    )
    .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(not_found_error(
            "config_center_get_not_found",
            "config",
            service_name.to_string(),
            request_id,
            trace_id,
        ));
    }
    let search_path = format!(
        "/configs/search?service_name_eq={}&limit=1",
        urlencoding::encode(service_name)
    );
    let search_resp = cert_service_request(
        state,
        reqwest::Method::GET,
        &search_path,
        None,
        client_ip,
        request_id,
        trace_id,
    )
    .await?;
    let mut results: Vec<serde_json::Value> = search_resp.json().await.map_err(|e| {
        dependency_internal_error("config_center_get_failed", request_id, trace_id, &e)
    })?;
    if let Some(config) = results.pop() {
        Ok(config)
    } else {
        Err(not_found_error(
            "config_center_get_not_found",
            "config",
            service_name.to_string(),
            request_id,
            trace_id,
        ))
    }
}
