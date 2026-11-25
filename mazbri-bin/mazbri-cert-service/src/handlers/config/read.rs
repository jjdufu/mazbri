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

fn wants_json_response(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| {
            accept
                .split(',')
                .map(|part| part.split(';').next().unwrap_or_default().trim())
                .any(|mime| mime.eq_ignore_ascii_case("application/json"))
        })
}

pub async fn get_config(
    State(state): State<ConfigState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> Response {
    let request_context = request_context(&headers);
    if let Ok(entry) = state.repo.get_by_service(&service, true).await {
        let version = state
            .repo
            .get_service_version(&service)
            .await
            .unwrap_or(1)
            .max(1);
        let version_header = version.to_string();
        let content_hash = entry.content_hash.clone().unwrap_or_default();
        info!(
            service = %service,
            version,
            content_length = entry.content.len(),
            "配置已获取"
        );
        if wants_json_response(&headers) {
            (
                StatusCode::OK,
                [
                    ("x-config-version", version_header.as_str()),
                    ("x-content-hash", content_hash.as_str()),
                ],
                Json(json!({
                    "service_name": entry.service_name,
                    "content": entry.content,
                    "content_hash": content_hash,
                    "version": version,
                    "is_sensitive": entry.is_sensitive,
                })),
            )
                .into_response()
        } else {
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE.as_str(), "text/plain; charset=utf-8"),
                    ("x-config-version", version_header.as_str()),
                    ("x-content-hash", content_hash.as_str()),
                ],
                entry.content,
            )
                .into_response()
        }
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
        service = ?query.service, request_id = %request_context.request_id,
        trace_id = %request_context.trace_id, "列出配置请求"
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
        count = entries.len(), request_id = %request_context.request_id,
        trace_id = %request_context.trace_id, "配置已列出"
    );
    (StatusCode::OK, Json(entries)).into_response()
}
