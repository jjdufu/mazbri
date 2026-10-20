use super::{
    cert_service_request, dependency_internal_error, ensure_service_name, ensure_toml,
    not_found_error, validation_error,
};
use crate::audit::{AuditAction, config_center_audit};
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::handlers::config_center::{CreateConfigRequest, UpdateConfigRequest};
use crate::middleware::auth;
use tracing::info;
pub async fn create_config(
    state: &AppState,
    claims: &auth::AdminClaims,
    payload: &CreateConfigRequest,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    ensure_service_name(&payload.service_name, request_id, trace_id)?;
    ensure_toml(&payload.content, request_id, trace_id)?;
    info!(
        "Creating config: {} (by {})",
        payload.service_name, claims.sub
    );
    let user_id = auth::extract_user_id(claims)?;
    let resource_id = payload.service_name.clone();
    let check_path = format!("/configs/{}", payload.service_name);
    let check_resp = cert_service_request(
        state,
        reqwest::Method::GET,
        &check_path,
        None,
        client_ip.clone(),
        request_id,
        trace_id,
    )
    .await?;
    if check_resp.status().is_success() {
        return Err(validation_error(
            "config_center_create_conflict",
            "配置已存在",
            request_id,
            trace_id,
        ));
    }
    let write_payload = serde_json::json!(
        { "content" : payload.content, "is_sensitive" : payload.is_sensitive
        .unwrap_or(false), "updated_by" : payload.updated_by, }
    );
    let resp = cert_service_request(
        state,
        reqwest::Method::PUT,
        &check_path,
        Some(write_payload),
        client_ip.clone(),
        request_id,
        trace_id,
    )
    .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".to_string());
        return Err(dependency_internal_error(
            "config_center_create_failed",
            request_id,
            trace_id,
            &format!("cert-service returned status {status}: {body}"),
        ));
    }
    let entry = resp.json::<serde_json::Value>().await.map_err(|e| {
        dependency_internal_error(
            "config_center_create_decode_failed",
            request_id,
            trace_id,
            &e,
        )
    })?;
    info!("Created config: {}", payload.service_name);
    state.audit.log(config_center_audit(
        user_id,
        AuditAction::ConfigCenterCreate,
        resource_id,
        Some(serde_json::json!({ "service_name" : payload.service_name, })),
        client_ip,
    ));
    Ok(entry)
}
pub async fn update_config(
    state: &AppState,
    claims: &auth::AdminClaims,
    service_name: &str,
    payload: &UpdateConfigRequest,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    ensure_service_name(service_name, request_id, trace_id)?;
    ensure_toml(&payload.content, request_id, trace_id)?;
    info!("Updating config: {} (by {})", service_name, claims.sub);
    let updated_by = payload.updated_by.as_deref().unwrap_or(&claims.sub);
    let user_id = auth::extract_user_id(claims)?;
    let resource_id = service_name.to_string();
    let write_payload = serde_json::json!(
        { "content" : payload.content, "updated_by" : updated_by, }
    );
    let path = format!("/configs/{service_name}");
    let resp = cert_service_request(
        state,
        reqwest::Method::PUT,
        &path,
        Some(write_payload),
        client_ip.clone(),
        request_id,
        trace_id,
    )
    .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".to_string());
        return Err(dependency_internal_error(
            "config_center_update_failed",
            request_id,
            trace_id,
            &format!("cert-service returned status {status}: {body}"),
        ));
    }
    let entry = resp.json::<serde_json::Value>().await.map_err(|e| {
        dependency_internal_error(
            "config_center_update_decode_failed",
            request_id,
            trace_id,
            &e,
        )
    })?;
    info!("Upserted config: {}", service_name);
    state.audit.log(config_center_audit(
        user_id,
        AuditAction::ConfigCenterUpdate,
        resource_id,
        Some(serde_json::json!({ "service_name" : service_name, })),
        client_ip,
    ));
    Ok(entry)
}
pub async fn delete_config(
    state: &AppState,
    claims: &auth::AdminClaims,
    service_name: &str,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    info!("Deleting config: {} (by {})", service_name, claims.sub);
    let user_id = auth::extract_user_id(claims)?;
    let resource_id = service_name.to_string();
    let path = format!("/configs/{service_name}");
    let resp = cert_service_request(
        state,
        reqwest::Method::DELETE,
        &path,
        None,
        client_ip.clone(),
        request_id,
        trace_id,
    )
    .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(not_found_error(
            "config_center_delete_not_found",
            "config",
            service_name.to_string(),
            request_id,
            trace_id,
        ));
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".to_string());
        return Err(dependency_internal_error(
            "config_center_delete_failed",
            request_id,
            trace_id,
            &format!("cert-service returned status {status}: {body}"),
        ));
    }
    info!("Deleted config: {}", service_name);
    state.audit.log(config_center_audit(
        user_id,
        AuditAction::ConfigCenterDelete,
        resource_id,
        Some(serde_json::json!({ "service_name" : service_name, })),
        client_ip,
    ));
    Ok(serde_json::json!({ "message" : "Config deleted successfully" }))
}
