use super::{ConfigState, log_config_error};
use crate::config_center::models::RollbackRequest;
use crate::handlers::helpers::request_context;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;
use tracing::info;
pub async fn rollback_config(
    State(state): State<ConfigState>,
    Path(service): Path<String>,
    headers: HeaderMap,
    Json(req): Json<RollbackRequest>,
) -> Response {
    let request_context = request_context(&headers);
    info!(
        service = % service, version = req.version, updated_by = % req.updated_by,
        request_id = % request_context.request_id, trace_id = % request_context.trace_id,
        "配置回滚请求"
    );
    let Ok(history_version) = state.repo.get_history_version(&service, req.version).await else {
        let message = format!("要回滚的版本不存在: {}/v{}", service, req.version);
        log_config_error(
            "config_rollback_target_not_found",
            mazbri_core::error_codes::NOT_FOUND,
            &request_context.request_id,
            &request_context.trace_id,
            &message,
        );
        return (
            StatusCode::NOT_FOUND,
            Json(json!(
                { "error" : mazbri_core::error_codes::NOT_FOUND, "message" :
                "指定的历史版本未找到，无法回滚" }
            )),
        )
            .into_response();
    };
    match state
        .repo
        .rollback(
            &service,
            &history_version.content,
            history_version.content_hash.as_deref(),
            &req.updated_by,
        )
        .await
    {
        Ok(entry) => {
            info!(
                service = % service, version = req.version,
                "配置回滚成功"
            );
            (StatusCode::OK, Json(entry)).into_response()
        }
        Err(e) => {
            let message = format!("回滚操作执行失败: {service} - {e}");
            log_config_error(
                "config_rollback_execution_failed",
                mazbri_core::error_codes::INTERNAL_ERROR,
                &request_context.request_id,
                &request_context.trace_id,
                &message,
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!(
                    { "error" : mazbri_core::error_codes::INTERNAL_ERROR, "message" :
                    "回滚执行失败" }
                )),
            )
                .into_response()
        }
    }
}
