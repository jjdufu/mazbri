use super::{
    CONFIG_IN_USE, CORE_SERVICE_CONFIG_PROTECTED, ConfigState, INTERNAL_SERVER_ERROR_MESSAGE,
    NOT_FOUND_MESSAGE, VALIDATION_ERROR_MESSAGE, is_core_protected_config, log_config_error,
};
use crate::config_center::ConfigError;
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

#[must_use]
pub(crate) fn is_unchanged_config_update(
    existing_content: &str,
    existing_sensitive: bool,
    next_content: &str,
    next_sensitive: bool,
) -> bool {
    existing_content == next_content && existing_sensitive == next_sensitive
}

/// 校验配置内容格式，失败时返回直接响应。
fn validate_toml_schema(content: &str, request_context: &RequestContext) -> Option<Response> {
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
    None
}

fn validate_target_service(service: &str, request_context: &RequestContext) -> Option<Response> {
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
    if let Some(response) = validate_toml_schema(&req.content, &request_context) {
        return response;
    }
    info!(
        service = % service, request_id = % request_context.request_id,
        trace_id = % request_context.trace_id, is_sensitive = ? req.is_sensitive,
        updated_by = % req.updated_by, "Upsert 配置请求"
    );
    match state.repo.get_by_service(&service, true).await {
        Ok(existing) => {
            let target_sensitive = req.is_sensitive.unwrap_or(existing.is_sensitive);
            if is_unchanged_config_update(
                &existing.content,
                existing.is_sensitive,
                &req.content,
                target_sensitive,
            ) {
                info!(
                    service = % service, request_id = % request_context.request_id,
                    trace_id = % request_context.trace_id, "配置内容未变更，幂等跳过数据库更新"
                );
                return (StatusCode::OK, Json(existing)).into_response();
            }
            match state
                .repo
                .update(&service, &req.content, &req.updated_by, target_sensitive)
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
        }
        Err(ConfigError::NotFound { .. }) => {
            if let Some(response) = validate_target_service(&service, &request_context) {
                return response;
            }
            let create_req = CreateConfigRequest {
                service_name: service,
                content: req.content,
                is_sensitive: req.is_sensitive.unwrap_or(false),
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
        Err(e) => upsert_internal_error("config_upsert_lookup_failed", &request_context, &e),
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
    if is_core_protected_config(&service) {
        let message = "平台核心配置受保护不可删除，请使用编辑配置或历史版本回滚";
        log_config_error(
            "config_delete_core_protected",
            CORE_SERVICE_CONFIG_PROTECTED,
            &request_context.request_id,
            &request_context.trace_id,
            &message,
        );
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": CORE_SERVICE_CONFIG_PROTECTED,
                "code": CORE_SERVICE_CONFIG_PROTECTED,
                "message": message,
            })),
        )
            .into_response();
    }
    match state.repo.get_by_service(&service, false).await {
        Ok(_) => {}
        Err(ConfigError::NotFound { .. }) => {
            let message = format!("配置不存在: {service}");
            log_config_error(
                "config_delete_not_found",
                mazbri_core::error_codes::NOT_FOUND,
                &request_context.request_id,
                &request_context.trace_id,
                &message,
            );
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": mazbri_core::error_codes::NOT_FOUND,
                    "message": NOT_FOUND_MESSAGE,
                })),
            )
                .into_response();
        }
        Err(e) => return upsert_internal_error("config_delete_lookup_failed", &request_context, &e),
    }
    match state.node_repo.has_online_node(&service).await {
        Ok(true) => {
            let message = "该服务仍存在在线运行实例，禁止删除生效配置";
            log_config_error(
                "config_delete_in_use",
                CONFIG_IN_USE,
                &request_context.request_id,
                &request_context.trace_id,
                &message,
            );
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": CONFIG_IN_USE,
                    "code": CONFIG_IN_USE,
                    "message": message,
                })),
            )
                .into_response();
        }
        Ok(false) => {}
        Err(e) => {
            return upsert_internal_error("config_delete_node_check_failed", &request_context, &e);
        }
    }
    match state.repo.delete(&service).await {
        Ok(()) => {
            info!(
                service = % service, request_id = % request_context.request_id,
                trace_id = % request_context.trace_id, "配置已删除"
            );
            (StatusCode::NO_CONTENT, "").into_response()
        }
        Err(ConfigError::NotFound { .. }) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": mazbri_core::error_codes::NOT_FOUND,
                "message": NOT_FOUND_MESSAGE,
            })),
        )
            .into_response(),
        Err(e) => upsert_internal_error("config_delete_failed", &request_context, &e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_center::ConfigRepository;
    use crate::node_registry::NodeRegistryRepository;
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::delete;
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Arc;
    use tower::ServiceExt;

    #[test]
    fn core_protected_covers_global_and_canonical_services_only() {
        assert!(is_core_protected_config("global"));
        assert!(is_core_protected_config("mazbri-gateway"));
        assert!(is_core_protected_config("mazbri-cert"));
        assert!(!is_core_protected_config("custom-ext-service"));
        assert!(!is_core_protected_config("nonexistent"));
    }

    #[test]
    fn unchanged_update_predicate_checks_content_and_sensitivity() {
        assert!(is_unchanged_config_update("port = 8080\n", false, "port = 8080\n", false));
        assert!(!is_unchanged_config_update("port = 8080\n", false, "port = 9090\n", false));
        assert!(!is_unchanged_config_update("port = 8080\n", false, "port = 8080\n", true));
    }

    #[tokio::test]
    async fn delete_core_config_returns_conflict_before_db_lookup() {
        let pool = Arc::new(
            PgPoolOptions::new()
                .acquire_timeout(std::time::Duration::from_millis(50))
                .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
                .expect("lazy pool"),
        );
        let state = ConfigState {
            repo: Arc::new(ConfigRepository::new(Arc::clone(&pool))),
            node_repo: Arc::new(NodeRegistryRepository::new(pool)),
        };
        let app = Router::new()
            .route("/configs/{service}", delete(delete_config))
            .with_state(state);
        for protected in ["global", "mazbri-gateway", "mazbri-mgmt"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("DELETE")
                        .uri(format!("/configs/{protected}"))
                        .body(Body::empty())
                        .expect("request"),
                )
                .await
                .expect("oneshot");
            assert_eq!(response.status(), StatusCode::CONFLICT, "{protected}");
            let bytes = axum::body::to_bytes(response.into_body(), 16_384)
                .await
                .expect("body");
            let text = String::from_utf8_lossy(&bytes);
            assert!(
                text.contains(CORE_SERVICE_CONFIG_PROTECTED),
                "expected CORE_SERVICE_CONFIG_PROTECTED for {protected}, got: {text}"
            );
        }
    }
}
