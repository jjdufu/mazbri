use crate::service_registry::{
    ServiceRepository,
    models::{CreateServiceRequest, ServiceValidationResponse, UpdateServiceRequest},
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info};
#[derive(Clone)]
pub struct ServiceRegistryState {
    pub repo: Arc<ServiceRepository>,
}
#[derive(Debug, Deserialize, Default)]
pub struct ListServicesQuery {
    pub status: Option<String>,
}
pub async fn list_services(
    State(state): State<ServiceRegistryState>,
    Query(params): Query<ListServicesQuery>,
) -> Response {
    let result = match params.status {
        Some(ref status) => state.repo.find_all_by_status(status).await,
        None => state.repo.find_all().await,
    };
    match result {
        Ok(services) => {
            info!(service_count = services.len(), "服务列表已获取");
            (StatusCode::OK, Json(services)).into_response()
        }
        Err(e) => {
            error!(error = % e, "获取服务列表失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!(
                    { "error" : "LIST_SERVICES_FAILED", "message" : e.to_string() }
                )),
            )
                .into_response()
        }
    }
}
pub async fn create_service(
    State(state): State<ServiceRegistryState>,
    Json(req): Json<CreateServiceRequest>,
) -> Response {
    info!(service_name = % req.service_name, "创建服务");
    match state.repo.create(&req).await {
        Ok(service) => {
            info!(
                id = % service.id, service_name = % service.service_name,
                "服务已创建"
            );
            (StatusCode::CREATED, Json(service)).into_response()
        }
        Err(e) => {
            error!(error = % e, service_name = % req.service_name, "创建服务失败");
            let status = if e.to_string().contains("already exists") {
                StatusCode::CONFLICT
            } else if e.to_string().contains("empty") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(json!(
                    { "error" : "CREATE_SERVICE_FAILED", "message" : e.to_string() }
                )),
            )
                .into_response()
        }
    }
}
pub async fn get_service(
    State(state): State<ServiceRegistryState>,
    Path(name): Path<String>,
) -> Response {
    match state.repo.find_by_name(&name).await {
        Ok(service) => {
            info!(service_name = % name, "服务已获取");
            (StatusCode::OK, Json(service)).into_response()
        }
        Err(e) => {
            error!(error = % e, service_name = % name, "获取服务失败");
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error" : "SERVICE_NOT_FOUND", "message" : e.to_string() })),
            )
                .into_response()
        }
    }
}
pub async fn update_service(
    State(state): State<ServiceRegistryState>,
    Path(name): Path<String>,
    Json(req): Json<UpdateServiceRequest>,
) -> Response {
    info!(service_name = % name, "更新服务");
    match state.repo.update(&name, &req).await {
        Ok(service) => {
            info!(service_name = % name, "服务已更新");
            (StatusCode::OK, Json(service)).into_response()
        }
        Err(e) => {
            error!(error = % e, service_name = % name, "更新服务失败");
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error" : "SERVICE_NOT_FOUND", "message" : e.to_string() })),
            )
                .into_response()
        }
    }
}
pub async fn delete_service(
    State(state): State<ServiceRegistryState>,
    Path(name): Path<String>,
) -> Response {
    info!(service_name = % name, "删除服务");
    match state.repo.soft_delete(&name).await {
        Ok(_) => {
            info!(service_name = % name, "服务已删除");
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            error!(error = % e, service_name = % name, "删除服务失败");
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error" : "SERVICE_NOT_FOUND", "message" : e.to_string() })),
            )
                .into_response()
        }
    }
}
pub async fn validate_service(
    State(state): State<ServiceRegistryState>,
    Path(name): Path<String>,
) -> Response {
    match state.repo.validate_service_name(&name).await {
        Ok(is_valid) => {
            let response = if is_valid {
                let service = state.repo.find_by_name(&name).await.ok();
                ServiceValidationResponse {
                    is_valid: true,
                    message: format!("服务 '{}' 已注册且状态为活跃", name),
                    service,
                }
            } else {
                ServiceValidationResponse {
                    is_valid: false,
                    message: format!("服务 '{}' 未注册或不是活跃状态", name),
                    service: None,
                }
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!(error = % e, service_name = % name, "验证服务失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!(
                    { "error" : "VALIDATE_SERVICE_FAILED", "message" : e.to_string()
                    }
                )),
            )
                .into_response()
        }
    }
}
