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
use mazbri_core::validation::validate_service_config;
use serde_json::json;
use tracing::{debug, info};
pub async fn upsert_config(
    State(state): State<ConfigState>,
    Path(service): Path<String>,
    headers: HeaderMap,
    Json(req): Json<UpdateConfigRequest>,
) -> Response {
    let request_context = request_context(&headers);
    if let Err(errs) = validate_service_config(&req.content) {
        let message = format!("配置内容校验失败: {:?}", errs);
        log_config_error(
            "config_upsert_schema_validation_failed",
            mazbri_core::error_codes::VALIDATION_ERROR,
            &request_context.request_id,
            &request_context.trace_id,
            &message,
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(json!(
                { "error" : mazbri_core::error_codes::VALIDATION_ERROR, "message" :
                format!("{}: {}", VALIDATION_ERROR_MESSAGE, message), "details" :
                errs }
            )),
        )
            .into_response();
    }
    match state
        .service_registry_repo
        .validate_service_name(&service)
        .await
    {
        Ok(true) => {
            debug!(service = % service, "服务名验证通过");
        }
        Ok(false) => {
            let message = format!("服务名 '{}' 不在注册中心或不是活跃状态", service);
            log_config_error(
                "config_upsert_service_validation_failed",
                mazbri_core::error_codes::VALIDATION_ERROR,
                &request_context.request_id,
                &request_context.trace_id,
                &message,
            );
            return (
                StatusCode::BAD_REQUEST,
                Json(json!(
                    { "error" : mazbri_core::error_codes::VALIDATION_ERROR, "message"
                    : message }
                )),
            )
                .into_response();
        }
        Err(e) => {
            log_config_error(
                "config_upsert_service_validation_failed",
                mazbri_core::error_codes::INTERNAL_ERROR,
                &request_context.request_id,
                &request_context.trace_id,
                &e,
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!(
                    { "error" : mazbri_core::error_codes::INTERNAL_ERROR, "message" :
                    INTERNAL_SERVER_ERROR_MESSAGE }
                )),
            )
                .into_response();
        }
    }
    info!(
        service = % service, request_id = % request_context.request_id,
        trace_id = % request_context.trace_id, is_sensitive = req.is_sensitive,
        updated_by = % req.updated_by, "Upsert 配置请求"
    );
    match state.repo.get_by_service(&service, false).await {
        Ok(_) => {
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
                Err(e) => {
                    log_config_error(
                        "config_upsert_update_failed",
                        mazbri_core::error_codes::INTERNAL_ERROR,
                        &request_context.request_id,
                        &request_context.trace_id,
                        &e,
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
            }
        }
        Err(_) => {
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
                Err(e) => {
                    log_config_error(
                        "config_upsert_create_failed",
                        mazbri_core::error_codes::INTERNAL_ERROR,
                        &request_context.request_id,
                        &request_context.trace_id,
                        &e,
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
            }
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
        Ok(_) => {
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
