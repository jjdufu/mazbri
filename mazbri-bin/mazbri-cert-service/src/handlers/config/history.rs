use super::{ConfigState, NOT_FOUND_MESSAGE, log_config_error};
use crate::handlers::helpers::request_context;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;
use tracing::info;
pub async fn get_config_history(
    State(state): State<ConfigState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> Response {
    let request_context = request_context(&headers);
    info!(
        service = % service, request_id = % request_context.request_id,
        trace_id = % request_context.trace_id, "获取配置历史记录请求"
    );
    match state.repo.get_history(&service, 100, 0).await {
        Ok(history) => {
            info!(
                service = % service, count = history.len(),
                "配置历史记录已获取"
            );
            (StatusCode::OK, Json(history)).into_response()
        }
        Err(e) => {
            let message = format!("获取配置历史记录失败: {service} - {e}");
            log_config_error(
                "config_history_get_failed",
                mazbri_core::error_codes::INTERNAL_ERROR,
                &request_context.request_id,
                &request_context.trace_id,
                &message,
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!(
                    { "error" : mazbri_core::error_codes::INTERNAL_ERROR, "message" :
                    "获取历史记录失败" }
                )),
            )
                .into_response()
        }
    }
}
pub async fn get_config_history_version(
    State(state): State<ConfigState>,
    Path((service, version)): Path<(String, i32)>,
    headers: HeaderMap,
) -> Response {
    let request_context = request_context(&headers);
    info!(
        service = % service, version = version, request_id = % request_context.request_id,
        trace_id = % request_context.trace_id, "获取特定版本的配置历史记录请求"
    );
    if let Ok(history) = state.repo.get_history_version(&service, version).await {
        info!(
            service = % service, version = version,
            "特定版本的配置历史记录已获取"
        );
        (StatusCode::OK, Json(history)).into_response()
    } else {
        let message = format!("历史版本不存在: {service}/v{version}");
        log_config_error(
            "config_history_version_not_found",
            mazbri_core::error_codes::NOT_FOUND,
            &request_context.request_id,
            &request_context.trace_id,
            &message,
        );
        (
            StatusCode::NOT_FOUND,
            Json(json!(
                { "error" : mazbri_core::error_codes::NOT_FOUND, "message" :
                NOT_FOUND_MESSAGE }
            )),
        )
            .into_response()
    }
}
