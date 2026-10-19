use super::ConfigState;
use crate::config_center::models::{BatchDeleteRequest, BatchUpdateRequest};
use crate::handlers::helpers::{logged_config_error_response, request_context};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use tracing::info;
fn status_for_batch_success(success: bool) -> StatusCode {
    if success {
        StatusCode::OK
    } else {
        StatusCode::PARTIAL_CONTENT
    }
}
pub async fn batch_delete_configs(
    State(state): State<ConfigState>,
    headers: HeaderMap,
    Json(req): Json<BatchDeleteRequest>,
) -> Response {
    let request_context = request_context(&headers);
    info!(
        dry_run = req.dry_run, request_id = % request_context.request_id,
        trace_id = % request_context.trace_id, "批量删除请求"
    );
    match state.repo.batch_delete(&req.filters, req.dry_run).await {
        Ok(result) => {
            info!(
                success = result.success,
                affected_count = result.affected_count,
                error_count = result.errors.len(),
                "批量删除完成"
            );
            (status_for_batch_success(result.success), Json(result)).into_response()
        }
        Err(e) => logged_config_error_response(
            "batch_delete_configs",
            mazbri_core::error_codes::INTERNAL_ERROR,
            Some(&request_context.request_id),
            Some(&request_context.trace_id),
            &e,
            Some(false),
            None,
        ),
    }
}
pub async fn batch_update_configs(
    State(state): State<ConfigState>,
    headers: HeaderMap,
    Json(req): Json<BatchUpdateRequest>,
) -> Response {
    let request_context = request_context(&headers);
    info!(
        dry_run = req.dry_run, operation_count = req.operations.len(),
        request_id = % request_context.request_id, trace_id = % request_context.trace_id,
        "批量更新请求"
    );
    match state
        .repo
        .batch_update(&req.filters, &req.operations, &req.updated_by, req.dry_run)
        .await
    {
        Ok(result) => {
            info!(
                success = result.success,
                affected_count = result.affected_count,
                error_count = result.errors.len(),
                "批量更新完成"
            );
            (status_for_batch_success(result.success), Json(result)).into_response()
        }
        Err(e) => logged_config_error_response(
            "batch_update_configs",
            mazbri_core::error_codes::INTERNAL_ERROR,
            Some(&request_context.request_id),
            Some(&request_context.trace_id),
            &e,
            Some(false),
            None,
        ),
    }
}
