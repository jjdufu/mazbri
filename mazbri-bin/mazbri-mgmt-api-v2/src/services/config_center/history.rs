use super::{cert_service_request, dependency_internal_error};
use crate::audit::{AuditAction, config_center_audit};
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::handlers::config_center::HistoryQueryParams;
use crate::middleware::auth;
use tracing::info;
pub async fn get_history(
    state: &AppState,
    service_name: &str,
    params: &HistoryQueryParams,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    let path = format!("/configs/{}/history", service_name);
    let resp = cert_service_request(
        state,
        reqwest::Method::GET,
        &path,
        None,
        client_ip,
        request_id,
        trace_id,
    )
    .await?;
    if !resp.status().is_success() {
        return Err(dependency_internal_error(
            "config_center_history_cert_service_failed",
            request_id,
            trace_id,
            &format!("Cert service returned error: {}", resp.status()),
        ));
    }
    let history: serde_json::Value = resp.json().await.map_err(|e| {
        dependency_internal_error(
            "config_center_history_parse_failed",
            request_id,
            trace_id,
            &e,
        )
    })?;
    let history_list = history.as_array().ok_or_else(|| {
        dependency_internal_error(
            "config_center_history_invalid_format",
            request_id,
            trace_id,
            &"Cert service history is not an array",
        )
    })?;
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20);
    let total = history_list.len();
    let start = (page.saturating_sub(1) * page_size) as usize;
    let end = (start + page_size as usize).min(total);
    let paged_history = if start < total {
        &history_list[start..end]
    } else {
        &[]
    };
    Ok(serde_json::json!(
        { "history" : paged_history, "total" : total, "page" : page, "page_size" :
        page_size }
    ))
}
pub async fn rollback(
    state: &AppState,
    claims: &auth::AdminClaims,
    service_name: &str,
    version: i32,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    info!(
        "Rolling back config: {} to version {} (by {})",
        service_name, version, claims.sub
    );
    let user_id = auth::extract_user_id(claims)?;
    let resource_id = service_name.to_string();
    let updated_by = format!("{} (rollback from v{})", claims.sub, version);
    let path = format!("/configs/{}/rollback", service_name);
    let body = serde_json::json!({ "version" : version, "updated_by" : updated_by, });
    let resp = cert_service_request(
        state,
        reqwest::Method::POST,
        &path,
        Some(body),
        client_ip.clone(),
        request_id,
        trace_id,
    )
    .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let error_msg = if status == reqwest::StatusCode::NOT_FOUND {
            format!("版本 v{} 不存在", version)
        } else {
            format!("Cert service 错误: {}", status)
        };
        return Err(dependency_internal_error(
            "config_center_rollback_cert_service_failed",
            request_id,
            trace_id,
            &error_msg,
        ));
    }
    let config: serde_json::Value = resp.json().await.map_err(|e| {
        dependency_internal_error(
            "config_center_rollback_parse_failed",
            request_id,
            trace_id,
            &e,
        )
    })?;
    info!(
        "Rolled back config: {} to version {}",
        service_name, version
    );
    state.audit.log(config_center_audit(
        user_id,
        AuditAction::ConfigCenterRollback,
        resource_id,
        Some(serde_json::json!(
            { "service_name" : service_name, "rollback_version" : version, }
        )),
        client_ip,
    ));
    Ok(config)
}
