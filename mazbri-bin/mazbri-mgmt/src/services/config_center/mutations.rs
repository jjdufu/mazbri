use super::{
    ConfigCenterCtx, cert_service_request, dependency_internal_error, ensure_service_name,
    ensure_toml, is_core_protected_config, log_config_center_error, not_found_error, query,
    sha256_hex, validation_error,
};
use crate::audit::{AuditAction, config_center_audit};
use crate::error::ApiError;
use crate::handlers::config_center::{CreateConfigRequest, UpdateConfigRequest};
use crate::middleware::auth;
use tracing::info;

pub async fn precheck_create_config(
    ctx: ConfigCenterCtx<'_>,
    payload: &CreateConfigRequest,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<(), ApiError> {
    ensure_service_name(&payload.service_name, request_id, trace_id)?;
    ensure_toml(&payload.content, request_id, trace_id)?;
    let check_path = format!("/configs/{}", urlencoding::encode(&payload.service_name));
    let check_resp = cert_service_request(
        ctx,
        reqwest::Method::GET,
        &check_path,
        None,
        client_ip,
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
    if check_resp.status() != reqwest::StatusCode::NOT_FOUND {
        return Err(dependency_internal_error(
            "config_center_create_precheck_failed",
            request_id,
            trace_id,
            &format!("cert-service returned status {}", check_resp.status()),
        ));
    }
    Ok(())
}

pub async fn create_config(
    ctx: ConfigCenterCtx<'_>,
    claims: &auth::AdminClaims,
    payload: &CreateConfigRequest,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    info!(
        "Creating config: {} (by {})",
        payload.service_name, claims.sub
    );
    let user_id = auth::extract_user_id(claims)?;
    let resource_id = payload.service_name.clone();
    let check_path = format!("/configs/{}", urlencoding::encode(&payload.service_name));
    let write_payload = serde_json::json!(
        { "content" : payload.content, "is_sensitive" : payload.is_sensitive
        .unwrap_or(false), "updated_by" : payload.updated_by, }
    );
    let resp = cert_service_request(
        ctx,
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
    let after_hash = entry
        .get("content_hash")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map_or_else(|| sha256_hex(&payload.content), str::to_string);
    ctx.audit.log(config_center_audit(
        user_id,
        AuditAction::ConfigCenterCreate,
        resource_id,
        Some(serde_json::json!({
            "service_name": payload.service_name,
            "after_hash": after_hash,
        })),
        client_ip,
    ));
    Ok(entry)
}

pub async fn precheck_update_config(
    ctx: ConfigCenterCtx<'_>,
    service_name: &str,
    payload: &UpdateConfigRequest,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<(), ApiError> {
    ensure_toml(&payload.content, request_id, trace_id)?;
    query::precheck_config_exists(ctx, service_name, client_ip, request_id, trace_id).await
}

pub async fn update_config(
    ctx: ConfigCenterCtx<'_>,
    claims: &auth::AdminClaims,
    service_name: &str,
    payload: &UpdateConfigRequest,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    ensure_toml(&payload.content, request_id, trace_id)?;
    info!("Updating config: {} (by {})", service_name, claims.sub);
    let encoded_name = urlencoding::encode(service_name);
    let path = format!("/configs/{encoded_name}");
    let current_resp = cert_service_request(
        ctx,
        reqwest::Method::GET,
        &path,
        None,
        client_ip.clone(),
        request_id,
        trace_id,
    )
    .await?;
    if current_resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(not_found_error(
            "config_center_update_not_found",
            "config",
            service_name.to_string(),
            request_id,
            trace_id,
        ));
    }
    let mut before_hash: Option<String> = None;
    if current_resp.status().is_success() {
        let existing_content = current_resp.text().await.map_err(|e| {
            dependency_internal_error(
                "config_center_update_read_current_failed",
                request_id,
                trace_id,
                &e,
            )
        })?;
        if existing_content == payload.content {
            info!(
                "Config {} content unchanged, skipping UPDATE to preserve config_history window",
                service_name
            );
            return query::get_config(ctx, service_name, client_ip, request_id, trace_id).await;
        }
        before_hash = Some(sha256_hex(&existing_content));
    }
    let updated_by = payload.updated_by.as_deref().unwrap_or(&claims.sub);
    let user_id = auth::extract_user_id(claims)?;
    let resource_id = service_name.to_string();
    let write_payload = serde_json::json!(
        { "content" : payload.content, "updated_by" : updated_by, }
    );
    let resp = cert_service_request(
        ctx,
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
    let after_hash = entry
        .get("content_hash")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map_or_else(|| sha256_hex(&payload.content), str::to_string);
    ctx.audit.log(config_center_audit(
        user_id,
        AuditAction::ConfigCenterUpdate,
        resource_id,
        Some(serde_json::json!({
            "service_name": service_name,
            "before_hash": before_hash,
            "after_hash": after_hash,
        })),
        client_ip,
    ));
    Ok(entry)
}

pub async fn precheck_delete_config(
    ctx: ConfigCenterCtx<'_>,
    service_name: &str,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<(), ApiError> {
    if is_core_protected_config(service_name) {
        let err = ApiError::CoreServiceConfigProtected;
        log_config_center_error(
            "config_center_delete_core_protected",
            "CORE_SERVICE_CONFIG_PROTECTED",
            request_id,
            trace_id,
            &err,
        );
        return Err(err);
    }
    let encoded_name = urlencoding::encode(service_name);
    let check_path = format!("/configs/{encoded_name}");
    let check_resp = cert_service_request(
        ctx,
        reqwest::Method::GET,
        &check_path,
        None,
        client_ip.clone(),
        request_id,
        trace_id,
    )
    .await?;
    if check_resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(not_found_error(
            "config_center_delete_not_found",
            "config",
            service_name.to_string(),
            request_id,
            trace_id,
        ));
    }
    if !check_resp.status().is_success() {
        return Err(dependency_internal_error(
            "config_center_delete_lookup_failed",
            request_id,
            trace_id,
            &format!("cert-service returned status {}", check_resp.status()),
        ));
    }
    let nodes_path = format!("/nodes/{encoded_name}");
    let nodes_resp = cert_service_request(
        ctx,
        reqwest::Method::GET,
        &nodes_path,
        None,
        client_ip,
        request_id,
        trace_id,
    )
    .await?;
    if !nodes_resp.status().is_success() {
        return Err(dependency_internal_error(
            "config_center_delete_nodes_check_failed",
            request_id,
            trace_id,
            &format!("cert-service nodes returned status {}", nodes_resp.status()),
        ));
    }
    let nodes: Vec<serde_json::Value> = nodes_resp.json().await.map_err(|e| {
        dependency_internal_error(
            "config_center_delete_nodes_parse_failed",
            request_id,
            trace_id,
            &e,
        )
    })?;
    if !nodes.is_empty() {
        let err = ApiError::ConfigInUse;
        log_config_center_error(
            "config_center_delete_in_use",
            "CONFIG_IN_USE",
            request_id,
            trace_id,
            &err,
        );
        return Err(err);
    }
    Ok(())
}

pub async fn delete_config(
    ctx: ConfigCenterCtx<'_>,
    claims: &auth::AdminClaims,
    service_name: &str,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    if is_core_protected_config(service_name) {
        return Err(ApiError::CoreServiceConfigProtected);
    }
    info!("Deleting config: {} (by {})", service_name, claims.sub);
    let user_id = auth::extract_user_id(claims)?;
    let resource_id = service_name.to_string();
    let path = format!("/configs/{}", urlencoding::encode(service_name));
    let before_hash = match cert_service_request(
        ctx,
        reqwest::Method::GET,
        &path,
        None,
        client_ip.clone(),
        request_id,
        trace_id,
    )
    .await
    {
        Ok(r) if r.status().is_success() => r.text().await.ok().map(|s| sha256_hex(&s)),
        _ => None,
    };
    let resp = cert_service_request(
        ctx,
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
    if resp.status() == reqwest::StatusCode::CONFLICT {
        let body = resp.text().await.unwrap_or_default();
        if body.contains("CORE_SERVICE_CONFIG_PROTECTED") {
            return Err(ApiError::CoreServiceConfigProtected);
        }
        return Err(ApiError::ConfigInUse);
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
    ctx.audit.log(config_center_audit(
        user_id,
        AuditAction::ConfigCenterDelete,
        resource_id,
        Some(serde_json::json!({
            "service_name": service_name,
            "before_hash": before_hash,
        })),
        client_ip,
    ));
    Ok(serde_json::json!({ "message" : "Config deleted successfully" }))
}
