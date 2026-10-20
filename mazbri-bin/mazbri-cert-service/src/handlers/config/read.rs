use super::{ConfigState, ListQuery, NOT_FOUND_MESSAGE, log_config_error};
use crate::handlers::helpers::request_context;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;
use tracing::info;
pub async fn get_config(
    State(state): State<ConfigState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> Response {
    let request_context = request_context(&headers);
    if let Ok(entry) = state.repo.get_by_service(&service, true).await {
        info!(
            service = % service, content_length = entry.content.len(),
            "配置已获取"
        );
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            entry.content,
        )
            .into_response()
    } else {
        let message = format!("配置不存在: {service}");
        log_config_error(
            "config_get_not_found",
            mazbri_core::error_codes::NOT_FOUND,
            &request_context.request_id,
            &request_context.trace_id,
            &message,
        );
        (
            StatusCode::NOT_FOUND,
            axum::Json(json!(
                { "error" : mazbri_core::error_codes::NOT_FOUND, "message" :
                NOT_FOUND_MESSAGE }
            )),
        )
            .into_response()
    }
}
pub async fn list_configs(
    State(state): State<ConfigState>,
    Query(query): Query<ListQuery>,
    headers: HeaderMap,
) -> Response {
    let request_context = request_context(&headers);
    info!(
        service = ? query.service, request_id = % request_context.request_id,
        trace_id = % request_context.trace_id, "列出配置请求"
    );
    let entries = if let Some(service) = &query.service {
        match state.repo.get_by_service(service, true).await {
            Ok(entry) => vec![entry],
            Err(_) => vec![],
        }
    } else {
        state.repo.list(1000, 0).await.unwrap_or_default()
    };
    info!(
        count = entries.len(), request_id = % request_context.request_id,
        trace_id = % request_context.trace_id, "配置已列出"
    );
    (StatusCode::OK, Json(entries)).into_response()
}
