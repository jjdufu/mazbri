use super::{
    ConfigState, INTERNAL_SERVER_ERROR_MESSAGE, VALIDATION_ERROR_MESSAGE, log_config_error,
};
use crate::config_center::models::{CreateConfigRequest, UpdateConfigRequest};
use crate::handlers::helpers::request_context;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use mazbri_core::request_context::RequestContext;
use mazbri_core::validation::validate_service_config;
use serde_json::json;
use tracing::info;
/// 校验失败时返回应直接响应的错误。
fn validate_upsert(
    service: &str,
    content: &str,
    request_context: &RequestContext,
) -> Option<Response> {
    if let Err(errs) = validate_service_config(content) {
        let message = format!("配置内容校验失败: {errs:?}");
        log_config_error(
            "config_upsert_schema_validation_failed",
            mazbri_core::error_codes::VALIDATION_ERROR,
            &request_context.request_id,
            &request_context.trace_id,
            &message,
        );
        return Some(
            (
                StatusCode::BAD_REQUEST,
                Json(json!(
                    { "error" : mazbri_core::error_codes::VALIDATION_ERROR, "message" :
                    format!("{}: {}", VALIDATION_ERROR_MESSAGE, message), "details" :
                    errs }
                )),
            )
                .into_response(),
        );
    }
    if let Err(e) = mazbri_core::validate_config_target(service) {
        let message = e.to_string();
        log_config_error(
            "config_upsert_service_validation_failed",
            mazbri_core::error_codes::VALIDATION_ERROR,
            &request_context.request_id,
            &request_context.trace_id,
            &message,
        );
        return Some(
            (
                StatusCode::BAD_REQUEST,
                Json(json!(
                    { "error" : mazbri_core::error_codes::VALIDATION_ERROR, "message"
                    : message }
                )),
            )
                .into_response(),
        );
    }
    None
}

fn upsert_internal_error(
    event: &'static str,
    request_context: &RequestContext,
    error: &dyn std::fmt::Display,
) -> Response {
    log_config_error(
        event,
        mazbri_core::error_codes::INTERNAL_ERROR,
        &request_context.request_id,
        &request_context.trace_id,
        error,
    );
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!(
            { "error" : mazbri_core::error_codes::INTERNAL_ERROR,
            "message" : INTERNAL_SERVER_ERROR_MESSAGE }
        )),
    )
        .into_response()
}

pub async fn upsert_config(
    State(state): State<ConfigState>,
    Path(service): Path<String>,
    headers: HeaderMap,
    Json(req): Json<UpdateConfigRequest>,
) -> Response {
    let request_context = request_context(&headers);
    if let Some(response) = validate_upsert(&service, &req.content, &request_context) {
        return response;
    }
    info!(
        service = % service, request_id = % request_context.request_id,
        trace_id = % request_context.trace_id, is_sensitive = req.is_sensitive,
        updated_by = % req.updated_by, "Upsert 配置请求"
    );
    if state.repo.get_by_service(&service, false).await.is_ok() {
        match state
            .repo
            .update(&service, &req.content, &req.updated_by, req.is_sensitive)
            .await
        {
            Ok(entry) => {
                info!(
                    service = % service, request_id = % request_context.request_id,
                    trace_id = % request_context.trace_id, "配置已更新"
                );
                (StatusCode::OK, Json(entry)).into_response()
            }
            Err(e) => upsert_internal_error("config_upsert_update_failed", &request_context, &e),
        }
    } else {
        let create_req = CreateConfigRequest {
            service_name: service,
            content: req.content,
            is_sensitive: req.is_sensitive,
            updated_by: req.updated_by,
        };
        match state.repo.create(&create_req).await {
            Ok(entry) => {
                info!(
                    service = % create_req.service_name, request_id = %
                    request_context.request_id, trace_id = % request_context.trace_id,
                    "配置已创建"
                );
                (StatusCode::CREATED, Json(entry)).into_response()
            }
            Err(e) => upsert_internal_error("config_upsert_create_failed", &request_context, &e),
        }
    }
}
pub async fn delete_config(
    State(state): State<ConfigState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> Response {
    let request_context = request_context(&headers);
    info!(
        service = % service, request_id = % request_context.request_id,
        trace_id = % request_context.trace_id, "删除配置请求"
    );
    match state.repo.delete(&service).await {
        Ok(()) => {
            info!(
                service = % service, request_id = % request_context.request_id,
                trace_id = % request_context.trace_id, "配置已删除"
            );
            (StatusCode::NO_CONTENT, "").into_response()
        }
        Err(e) => {
            log_config_error(
                "config_delete_failed",
                mazbri_core::error_codes::INTERNAL_ERROR,
                &request_context.request_id,
                &request_context.trace_id,
                &e,
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!(
                    { "error" : mazbri_core::error_codes::INTERNAL_ERROR, "message" :
                    INTERNAL_SERVER_ERROR_MESSAGE }
                )),
            )
                .into_response()
        }
    }
}
