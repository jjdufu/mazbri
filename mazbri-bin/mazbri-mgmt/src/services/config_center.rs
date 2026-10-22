use crate::error::ApiError;
use crate::handlers::AppState;
use crate::handlers::config_center::{
    ConfigListParams, CreateConfigRequest, ExportParams, HistoryQueryParams, ImportParams,
    NodeListParams, ServiceListParams, TomlErrorInfo, UpdateConfigRequest, ValidateTomlRequest,
    ValidateTomlResponse, parse_toml_error_position, validate_toml,
};
use crate::middleware::auth;
use std::fmt::Display;
mod history;
mod import_export;
mod metadata;
mod mutations;
mod query;
const CONFIG_CENTER_INTERNAL_MESSAGE: &str = "操作失败";
const MAX_EXTRACTED_FILE_SIZE: u64 = 50 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: f64 = 100.0;
const MAX_TOTAL_EXTRACTED_SIZE: u64 = 500 * 1024 * 1024;
fn severity_for_error_code(error_code: &str) -> &'static str {
    match error_code {
        "VALIDATION_ERROR" | "NOT_FOUND" => "warn",
        _ => "error",
    }
}
fn log_config_center_error(
    event: &'static str,
    error_code: &'static str,
    request_id: &str,
    trace_id: &str,
    error: &dyn Display,
) {
    let severity = severity_for_error_code(error_code);
    match severity {
        "warn" => {
            tracing::warn!(
                service = mazbri_core::ServiceName::Mgmt.as_str(), event = event, severity = severity,
                error_code = error_code, request_id = request_id, trace_id = trace_id,
                error = % error, "config center request failed"
            );
        }
        _ => {
            tracing::error!(
                service = mazbri_core::ServiceName::Mgmt.as_str(), event = event, severity = severity,
                error_code = error_code, request_id = request_id, trace_id = trace_id,
                error = % error, "config center request failed"
            );
        }
    }
}
fn validation_error(
    event: &'static str,
    message: impl Into<String>,
    request_id: &str,
    trace_id: &str,
) -> ApiError {
    let error = ApiError::validation(message);
    log_config_center_error(event, "VALIDATION_ERROR", request_id, trace_id, &error);
    error
}
fn not_found_error(
    event: &'static str,
    resource: impl Into<String>,
    id: impl Into<String>,
    request_id: &str,
    trace_id: &str,
) -> ApiError {
    let error = ApiError::not_found(resource.into(), id.into());
    log_config_center_error(event, "NOT_FOUND", request_id, trace_id, &error);
    error
}
fn dependency_internal_error(
    event: &'static str,
    request_id: &str,
    trace_id: &str,
    dependency_error: &dyn Display,
) -> ApiError {
    log_config_center_error(
        event,
        "INTERNAL_ERROR",
        request_id,
        trace_id,
        dependency_error,
    );
    ApiError::internal(CONFIG_CENTER_INTERNAL_MESSAGE.to_string())
}
fn ensure_service_name(
    service_name: &str,
    request_id: &str,
    trace_id: &str,
) -> Result<(), ApiError> {
    mazbri_core::validate_config_target(service_name).map_err(|e| {
        validation_error(
            "config_center_service_validation_failed",
            e.to_string(),
            request_id,
            trace_id,
        )
    })
}
pub(super) async fn cert_service_request(
    state: &AppState,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<reqwest::Response, ApiError> {
    let url = format!("{}{}", state.cert_service.url, path);
    let mut request_builder = state.cert_http_client.request(method, &url);
    request_builder = request_builder.bearer_auth(&state.cert_service.token);
    if let Some(ip) = client_ip {
        request_builder = request_builder.header("x-forwarded-for", ip);
    }
    if let Some(body) = body {
        request_builder = request_builder.json(&body);
    }
    request_builder.send().await.map_err(|e| {
        dependency_internal_error(
            "config_center_cert_service_request_failed",
            request_id,
            trace_id,
            &e,
        )
    })
}
fn ensure_toml(content: &str, request_id: &str, trace_id: &str) -> Result<(), ApiError> {
    validate_toml(content).map_err(|_| {
        validation_error(
            "config_center_toml_validation_failed",
            "TOML 配置内容不合法",
            request_id,
            trace_id,
        )
    })
}
fn validate_zip_path(name: &str) -> Result<(), String> {
    if name.contains('\0') {
        return Err(format!("Path contains null byte: {name}"));
    }
    if name.contains("..") {
        return Err(format!("Path traversal detected: {name}"));
    }
    if name.starts_with('/') {
        return Err(format!("Absolute path not allowed: {name}"));
    }
    if name.len() >= 2 && name.chars().nth(1) == Some(':') {
        return Err(format!("Absolute path not allowed: {name}"));
    }
    if name.contains('\\') {
        return Err(format!("Backslash not allowed in path: {name}"));
    }
    Ok(())
}
fn validate_zip_entry(
    name: &str,
    compressed_size: u64,
    uncompressed_size: u64,
) -> Result<(), String> {
    validate_zip_path(name)?;
    let ratio = if compressed_size == 0 {
        if uncompressed_size > 0 {
            f64::INFINITY
        } else {
            1.0
        }
    } else {
        mazbri_core::num::u64_to_f64(uncompressed_size)
            / mazbri_core::num::u64_to_f64(compressed_size)
    };
    if ratio > MAX_COMPRESSION_RATIO {
        return Err(format!(
            "Suspicious compression ratio {ratio:.1}:1 in {name} (max allowed: {MAX_COMPRESSION_RATIO:.1}:1). Possible ZIP bomb."
        ));
    }
    if uncompressed_size > MAX_EXTRACTED_FILE_SIZE {
        return Err(format!(
            "Extracted file too large: {name} ({uncompressed_size} bytes, max: {MAX_EXTRACTED_FILE_SIZE} bytes)"
        ));
    }
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_configs(
    state: &AppState,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
    params: &ConfigListParams,
) -> Result<serde_json::Value, ApiError> {
    query::list_configs(state, client_ip, request_id, trace_id, params).await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_config(
    state: &AppState,
    service_name: &str,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    query::get_config(state, service_name, client_ip, request_id, trace_id).await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_config(
    state: &AppState,
    claims: &auth::AdminClaims,
    payload: &CreateConfigRequest,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    mutations::create_config(state, claims, payload, client_ip, request_id, trace_id).await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_config(
    state: &AppState,
    claims: &auth::AdminClaims,
    service_name: &str,
    payload: &UpdateConfigRequest,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    mutations::update_config(
        state,
        claims,
        service_name,
        payload,
        client_ip,
        request_id,
        trace_id,
    )
    .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_config(
    state: &AppState,
    claims: &auth::AdminClaims,
    service_name: &str,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    mutations::delete_config(state, claims, service_name, client_ip, request_id, trace_id).await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_history(
    state: &AppState,
    service_name: &str,
    params: &HistoryQueryParams,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    history::get_history(state, service_name, params, client_ip, request_id, trace_id).await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn rollback(
    state: &AppState,
    claims: &auth::AdminClaims,
    service_name: &str,
    version: i32,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    history::rollback(
        state,
        claims,
        service_name,
        version,
        client_ip,
        request_id,
        trace_id,
    )
    .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn export_configs(
    state: &AppState,
    claims: &auth::AdminClaims,
    client_ip: Option<String>,
    params: &ExportParams,
    request_id: &str,
    trace_id: &str,
) -> Result<axum::response::Response, ApiError> {
    import_export::export_configs(state, claims, client_ip, params, request_id, trace_id).await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn import_configs(
    state: &AppState,
    claims: &auth::AdminClaims,
    client_ip: Option<String>,
    params: &ImportParams,
    body: axum::body::Body,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    import_export::import_configs(state, claims, client_ip, params, body, request_id, trace_id)
        .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_services(
    state: &AppState,
    params: &ServiceListParams,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    metadata::list_services(state, params, client_ip, request_id, trace_id).await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_nodes(
    state: &AppState,
    params: &NodeListParams,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    metadata::list_nodes(state, params, client_ip, request_id, trace_id).await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_toml_endpoint(
    payload: &ValidateTomlRequest,
) -> Result<serde_json::Value, ApiError> {
    if payload.content.trim().is_empty() {
        return Ok(serde_json::to_value(ValidateTomlResponse {
            valid: true,
            error: None,
        })
        .unwrap_or_default());
    }
    match payload.content.parse::<toml_edit::DocumentMut>() {
        Ok(_) => Ok(serde_json::to_value(ValidateTomlResponse {
            valid: true,
            error: None,
        })
        .unwrap_or_default()),
        Err(e) => {
            let (line, column) = parse_toml_error_position(&e, &payload.content);
            Ok(serde_json::to_value(ValidateTomlResponse {
                valid: false,
                error: Some(TomlErrorInfo {
                    line,
                    column,
                    message: "TOML 语法错误".to_string(),
                }),
            })
            .unwrap_or_default())
        }
    }
}
