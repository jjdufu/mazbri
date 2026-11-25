use super::{ConfigState, VALIDATION_ERROR_MESSAGE, log_config_error};
use crate::config_center::models::{ConfigHistory, RollbackRequest};
use crate::config_center::validator::validate_toml;
use crate::handlers::helpers::request_context;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use mazbri_core::validation::validate_service_config;
use serde_json::json;
use tracing::info;

async fn decrypt_and_validate_history(
    state: &ConfigState,
    service: &str,
    target_version: i32,
    history_version: &ConfigHistory,
    request_id: &str,
    trace_id: &str,
) -> Result<String, Box<Response>> {
    let plaintext_toml = state
        .repo
        .decrypt_history_content(history_version)
        .await
        .map_err(|e| {
            let message = format!("历史版本解密失败: {service}/v{target_version} - {e}");
            log_config_error(
                "config_rollback_decrypt_failed",
                mazbri_core::error_codes::INTERNAL_ERROR,
                request_id,
                trace_id,
                &message,
            );
            Box::new(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": mazbri_core::error_codes::INTERNAL_ERROR,
                        "message": "历史敏感配置解密失败，无法回滚"
                    })),
                )
                    .into_response(),
            )
        })?;

    if let Err(e) = validate_toml(&plaintext_toml) {
        let message = format!("历史配置 TOML 语法校验失败: {e}");
        log_config_error(
            "config_rollback_toml_validation_failed",
            mazbri_core::error_codes::VALIDATION_ERROR,
            request_id,
            trace_id,
            &message,
        );
        return Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": mazbri_core::error_codes::VALIDATION_ERROR,
                    "message": format!("{VALIDATION_ERROR_MESSAGE}: {message}")
                })),
            )
                .into_response(),
        ));
    }

    if let Err(errs) = validate_service_config(&plaintext_toml) {
        let message = format!("历史配置业务规则校验失败: {errs:?}");
        log_config_error(
            "config_rollback_schema_validation_failed",
            mazbri_core::error_codes::VALIDATION_ERROR,
            request_id,
            trace_id,
            &message,
        );
        return Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": mazbri_core::error_codes::VALIDATION_ERROR,
                    "message": format!("{VALIDATION_ERROR_MESSAGE}: {message}"),
                    "details": errs
                })),
            )
                .into_response(),
        ));
    }

    Ok(plaintext_toml)
}

pub async fn rollback_config(
    State(state): State<ConfigState>,
    Path(service): Path<String>,
    headers: HeaderMap,
    Json(req): Json<RollbackRequest>,
) -> Response {
    let ctx = request_context(&headers);
    info!(
        service = %service, version = req.version, updated_by = %req.updated_by,
        request_id = %ctx.request_id, trace_id = %ctx.trace_id,
        "配置回滚请求"
    );
    let Ok(history_version) = state.repo.get_history_version(&service, req.version).await else {
        let message = format!("要回滚的版本不存在: {service}/v{}", req.version);
        log_config_error(
            "config_rollback_target_not_found",
            mazbri_core::error_codes::NOT_FOUND,
            &ctx.request_id,
            &ctx.trace_id,
            &message,
        );
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": mazbri_core::error_codes::NOT_FOUND,
                "message": "指定的历史版本未找到，无法回滚"
            })),
        )
            .into_response();
    };

    let plaintext_toml = match decrypt_and_validate_history(
        &state,
        &service,
        req.version,
        &history_version,
        &ctx.request_id,
        &ctx.trace_id,
    )
    .await
    {
        Ok(toml_text) => toml_text,
        Err(resp) => return *resp,
    };

    match state
        .repo
        .rollback(
            &service,
            &plaintext_toml,
            history_version.is_sensitive,
            &req.updated_by,
        )
        .await
    {
        Ok(entry) => {
            info!(
                service = %service, version = req.version, is_sensitive = history_version.is_sensitive,
                "配置回滚成功"
            );
            (StatusCode::OK, Json(entry)).into_response()
        }
        Err(e) => {
            let message = format!("回滚操作执行失败: {service} - {e}");
            log_config_error(
                "config_rollback_execution_failed",
                mazbri_core::error_codes::INTERNAL_ERROR,
                &ctx.request_id,
                &ctx.trace_id,
                &message,
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": mazbri_core::error_codes::INTERNAL_ERROR,
                    "message": "回滚执行失败"
                })),
            )
                .into_response()
        }
    }
}
