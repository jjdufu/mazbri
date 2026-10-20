use super::ConfigState;
use crate::config_center::models::{ConfigCategoryRequest, ConfigSearchQuery, ConfigTagsRequest};
use crate::handlers::helpers::{logged_config_error_response, request_context};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;
use tracing::info;
pub async fn add_config_tags(
    State(state): State<ConfigState>,
    Path(service): Path<String>,
    headers: HeaderMap,
    Json(req): Json<ConfigTagsRequest>,
) -> Response {
    let request_context = request_context(&headers);
    info!(
        service = % service, tag_count = req.tag_names.len(), request_id = %
        request_context.request_id, trace_id = % request_context.trace_id,
        "为配置添加标签"
    );
    match state.repo.get_by_service(&service, false).await {
        Ok(entry) => {
            match state
                .repo
                .add_tags_to_config(entry.id, &req.tag_names)
                .await
            {
                Ok(()) => {
                    info!(service = % service, "标签已添加到配置");
                    (StatusCode::OK, Json(json!({ "success" : true }))).into_response()
                }
                Err(e) => logged_config_error_response(
                    "add_config_tags",
                    mazbri_core::error_codes::INTERNAL_ERROR,
                    Some(&request_context.request_id),
                    Some(&request_context.trace_id),
                    &e,
                    Some(false),
                    None,
                ),
            }
        }
        Err(e) => logged_config_error_response(
            "add_config_tags",
            mazbri_core::error_codes::NOT_FOUND,
            Some(&request_context.request_id),
            Some(&request_context.trace_id),
            &e,
            Some(false),
            Some(format!("未找到配置 {service}")),
        ),
    }
}
pub async fn remove_config_tags(
    State(state): State<ConfigState>,
    Path(service): Path<String>,
    headers: HeaderMap,
    Json(req): Json<ConfigTagsRequest>,
) -> Response {
    let request_context = request_context(&headers);
    info!(
        service = % service, tag_count = req.tag_names.len(), request_id = %
        request_context.request_id, trace_id = % request_context.trace_id,
        "从配置移除标签"
    );
    match state.repo.get_by_service(&service, false).await {
        Ok(entry) => {
            match state
                .repo
                .remove_tags_from_config(entry.id, &req.tag_names)
                .await
            {
                Ok(()) => {
                    info!(service = % service, "标签已从配置移除");
                    (StatusCode::OK, Json(json!({ "success" : true }))).into_response()
                }
                Err(e) => logged_config_error_response(
                    "remove_config_tags",
                    mazbri_core::error_codes::INTERNAL_ERROR,
                    Some(&request_context.request_id),
                    Some(&request_context.trace_id),
                    &e,
                    Some(false),
                    None,
                ),
            }
        }
        Err(e) => logged_config_error_response(
            "remove_config_tags",
            mazbri_core::error_codes::NOT_FOUND,
            Some(&request_context.request_id),
            Some(&request_context.trace_id),
            &e,
            Some(false),
            Some(format!("未找到配置 {service}")),
        ),
    }
}
pub async fn set_config_category(
    State(state): State<ConfigState>,
    Path(service): Path<String>,
    headers: HeaderMap,
    Json(req): Json<ConfigCategoryRequest>,
) -> Response {
    let request_context = request_context(&headers);
    info!(
        service = % service, category_id = ? req.category_id, request_id = %
        request_context.request_id, trace_id = % request_context.trace_id,
        "为配置设置分类"
    );
    match state.repo.get_by_service(&service, false).await {
        Ok(entry) => {
            match state
                .repo
                .set_config_category(entry.id, req.category_id)
                .await
            {
                Ok(()) => {
                    info!(service = % service, "分类已设置到配置");
                    (StatusCode::OK, Json(json!({ "success" : true }))).into_response()
                }
                Err(e) => logged_config_error_response(
                    "set_config_category",
                    mazbri_core::error_codes::INTERNAL_ERROR,
                    Some(&request_context.request_id),
                    Some(&request_context.trace_id),
                    &e,
                    Some(false),
                    None,
                ),
            }
        }
        Err(e) => logged_config_error_response(
            "set_config_category",
            mazbri_core::error_codes::NOT_FOUND,
            Some(&request_context.request_id),
            Some(&request_context.trace_id),
            &e,
            Some(false),
            Some(format!("未找到配置 {service}")),
        ),
    }
}
pub async fn search_configs(
    State(state): State<ConfigState>,
    headers: HeaderMap,
    Query(query): Query<ConfigSearchQuery>,
) -> Response {
    let request_context = request_context(&headers);
    info!(
        tags = ? query.tags, category = ? query.category, service_name = ? query.service_name,
        request_id = % request_context.request_id, trace_id = % request_context.trace_id,
        "配置搜索请求"
    );
    match state.repo.search_configs(&query).await {
        Ok(results) => {
            info!(result_count = results.len(), "配置搜索完成");
            (StatusCode::OK, Json(results)).into_response()
        }
        Err(e) => logged_config_error_response(
            "search_configs",
            mazbri_core::error_codes::INTERNAL_ERROR,
            Some(&request_context.request_id),
            Some(&request_context.trace_id),
            &e,
            Some(false),
            None,
        ),
    }
}
