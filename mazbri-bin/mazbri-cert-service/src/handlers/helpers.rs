use crate::config_center::ConfigError;
use crate::error::CertError;
use axum::{
    Json,
    extract::Multipart,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_core::error_codes;
pub use mazbri_core::request_context::request_context;
use serde::Serialize;
use serde_json::json;
use std::fmt::Display;
use tracing::{error, info, warn};
const NOT_FOUND_MESSAGE: &str = "资源未找到";
const VALIDATION_ERROR_MESSAGE: &str = "请求参数校验失败";
const INTERNAL_SERVER_ERROR_MESSAGE: &str = "内部服务器错误";
fn normalized_error_code(code: &str, err: &ConfigError) -> &'static str {
    if code.contains("NOT_FOUND") || matches!(err, ConfigError::NotFound { .. }) {
        error_codes::NOT_FOUND
    } else if code.contains("VALIDATION") || code.contains("EXISTS") {
        error_codes::VALIDATION_ERROR
    } else {
        error_codes::INTERNAL_ERROR
    }
}
fn severity_for_error_code(code: &str) -> &'static str {
    match code {
        error_codes::NOT_FOUND | error_codes::VALIDATION_ERROR => "warn",
        _ => "error",
    }
}
fn status_for_error_code(code: &str) -> StatusCode {
    match code {
        error_codes::NOT_FOUND => StatusCode::NOT_FOUND,
        error_codes::VALIDATION_ERROR => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
fn controlled_message(code: &str) -> &'static str {
    match code {
        error_codes::NOT_FOUND => NOT_FOUND_MESSAGE,
        error_codes::VALIDATION_ERROR => VALIDATION_ERROR_MESSAGE,
        _ => INTERNAL_SERVER_ERROR_MESSAGE,
    }
}
fn config_error_response_body(
    error_code: &str,
    message: String,
    success: Option<bool>,
) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    if let Some(success) = success {
        body.insert("success".to_string(), serde_json::Value::Bool(success));
    }
    body.insert(
        "error".to_string(),
        serde_json::Value::String(String::from(error_code)),
    );
    body.insert("message".to_string(), serde_json::Value::String(message));
    serde_json::Value::Object(body)
}
fn resolve_trace_context(request_id: Option<&str>, trace_id: Option<&str>) -> (String, String) {
    let context = mazbri_core::request_context::request_context_values(request_id, trace_id);
    (context.request_id, context.trace_id)
}

#[derive(Debug, Default)]
pub struct AuthorizationImportForm {
    pub passphrase: Option<String>,
    pub activate_now: Option<bool>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn parse_authorization_import_form(
    mut multipart: Multipart,
) -> Result<(Vec<u8>, AuthorizationImportForm), CertError> {
    let mut package_file: Option<Vec<u8>> = None;
    let mut form = AuthorizationImportForm::default();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| CertError::InvalidRequest(format!("Failed to read multipart form: {e}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "package_file" | "package" => {
                let bytes = field.bytes().await.map_err(|e| {
                    CertError::InvalidRequest(format!("Failed to read package file: {e}"))
                })?;
                package_file = Some(bytes.to_vec());
            }
            "passphrase" => {
                let value = field.text().await.map_err(|e| {
                    CertError::InvalidRequest(format!("Failed to read passphrase: {e}"))
                })?;
                form.passphrase = Some(value);
            }
            "activate_now" => {
                let value = field.text().await.map_err(|e| {
                    CertError::InvalidRequest(format!("Failed to read activate_now: {e}"))
                })?;
                form.activate_now = Some(matches!(value.trim(), "1" | "true" | "yes"));
            }
            _ => {}
        }
    }
    let package_file = package_file
        .ok_or_else(|| CertError::InvalidRequest("package_file is required".to_string()))?;
    if form
        .passphrase
        .as_ref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err(CertError::InvalidRequest(
            "passphrase is required".to_string(),
        ));
    }
    Ok((package_file, form))
}

fn log_config_entry_error(
    action: &str,
    error_code: &str,
    request_id: &str,
    trace_id: &str,
    err: &ConfigError,
) {
    let severity = severity_for_error_code(error_code);
    match severity {
        "warn" => {
            warn!(
                service = mazbri_core::ServiceName::Cert.as_str(), event = "config_handler_failed",
                severity = severity, error_code = error_code, request_id = request_id,
                trace_id = trace_id, action = action, error = % err, "{} failed", action
            );
        }
        _ => {
            error!(
                service = mazbri_core::ServiceName::Cert.as_str(), event = "config_handler_failed",
                severity = severity, error_code = error_code, request_id = request_id,
                trace_id = trace_id, action = action, error = % err, "{} failed", action
            );
        }
    }
}
#[must_use]
pub fn controlled_config_error_response(
    error_code: &str,
    success: Option<bool>,
    message: Option<String>,
) -> Response {
    let message = message.unwrap_or_else(|| controlled_message(error_code).to_string());
    (
        status_for_error_code(error_code),
        Json(config_error_response_body(error_code, message, success)),
    )
        .into_response()
}
pub fn logged_config_error_response(
    action: &str,
    error_code: &str,
    request_id: Option<&str>,
    trace_id: Option<&str>,
    err: &dyn Display,
    success: Option<bool>,
    message: Option<String>,
) -> Response {
    let (request_id, trace_id) = resolve_trace_context(request_id, trace_id);
    let severity = severity_for_error_code(error_code);
    match severity {
        "warn" => {
            warn!(
                service = mazbri_core::ServiceName::Cert.as_str(), event = "config_handler_failed",
                severity = severity, error_code = error_code, request_id = request_id,
                trace_id = trace_id, action = action, error = % err, "{} failed", action
            );
        }
        _ => {
            error!(
                service = mazbri_core::ServiceName::Cert.as_str(), event = "config_handler_failed",
                severity = severity, error_code = error_code, request_id = request_id,
                trace_id = trace_id, action = action, error = % err, "{} failed", action
            );
        }
    }
    controlled_config_error_response(error_code, success, message)
}
pub async fn with_config_entry<F, Fut, T>(
    action: &str,
    error_code: &str,
    success_status: StatusCode,
    f: F,
) -> Response
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, ConfigError>>,
    T: Serialize,
{
    with_config_entry_ctx(action, error_code, success_status, None, None, f).await
}
pub async fn with_config_entry_ctx<F, Fut, T>(
    action: &str,
    error_code: &str,
    success_status: StatusCode,
    request_id: Option<&str>,
    trace_id: Option<&str>,
    f: F,
) -> Response
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, ConfigError>>,
    T: Serialize,
{
    match f().await {
        Ok(data) => (success_status, Json(data)).into_response(),
        Err(e) => {
            let code = normalized_error_code(error_code, &e);
            let (request_id, trace_id) = resolve_trace_context(request_id, trace_id);
            log_config_entry_error(action, code, &request_id, &trace_id, &e);
            (
                status_for_error_code(code),
                Json(json!({ "error" : code, "message" : controlled_message(code), })),
            )
                .into_response()
        }
    }
}
pub async fn with_config_entry_unit<F, Fut>(action: &str, error_code: &str, f: F) -> Response
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), ConfigError>>,
{
    with_config_entry_unit_ctx(action, error_code, None, None, f).await
}
pub async fn with_config_entry_unit_ctx<F, Fut>(
    action: &str,
    error_code: &str,
    request_id: Option<&str>,
    trace_id: Option<&str>,
    f: F,
) -> Response
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), ConfigError>>,
{
    match f().await {
        Ok(()) => {
            info!(action = action, "{} succeeded", action);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            let code = normalized_error_code(error_code, &e);
            let (request_id, trace_id) = resolve_trace_context(request_id, trace_id);
            log_config_entry_error(action, code, &request_id, &trace_id, &e);
            (
                status_for_error_code(code),
                Json(json!({ "error" : code, "message" : controlled_message(code), })),
            )
                .into_response()
        }
    }
}
