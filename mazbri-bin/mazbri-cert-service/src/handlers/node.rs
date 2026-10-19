use crate::config_center::ConfigRepository;
use crate::node_registry::{
    HeartbeatData, NodeRegistryRepository, RegisterNodeData, RegisteredNode,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, error, info};
#[derive(Clone)]
pub struct NodeState {
    pub repo: Arc<NodeRegistryRepository>,
    pub config_repo: Arc<ConfigRepository>,
}
#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    pub service_name: String,
    pub config_version: i32,
    pub ip: String,
    pub port: u16,
}
#[derive(Debug, serde::Serialize)]
pub struct HeartbeatResponse {
    pub success: bool,
    pub error_message: String,
    pub config_changed: bool,
    pub latest_version: i32,
}
pub async fn register_node(
    State(state): State<NodeState>,
    Json(req): Json<RegisterNodeData>,
) -> Response {
    match state.repo.register(&req).await {
        Ok(node) => {
            info!(
                service = % node.service_name, node_id = % node.id,
                "节点已注册"
            );
            (StatusCode::OK, Json(node)).into_response()
        }
        Err(e) => {
            error!(error = % e, "注册节点失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error" : "DATABASE_ERROR", "message" : e.to_string() })),
            )
                .into_response()
        }
    }
}
pub async fn heartbeat(
    State(state): State<NodeState>,
    Json(req): Json<HeartbeatRequest>,
) -> Response {
    let latest_version = match state
        .config_repo
        .get_service_version(&req.service_name)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            error!(error = % e, "获取服务版本失败");
            0
        }
    };
    let config_changed = req.config_version < latest_version;
    if config_changed {
        debug!(
            service = % req.service_name, client_version = req.config_version,
            server_version = latest_version, "心跳检测到配置变更"
        );
    }
    let data = HeartbeatData {
        service_name: req.service_name,
        config_version: req.config_version,
    };
    match state.repo.heartbeat(&data, &req.ip, req.port).await {
        Ok(true) => (
            StatusCode::OK,
            Json(HeartbeatResponse {
                success: true,
                error_message: String::new(),
                config_changed,
                latest_version,
            }),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::OK,
            Json(HeartbeatResponse {
                success: false,
                error_message: "Node not registered".to_string(),
                config_changed: false,
                latest_version: 0,
            }),
        )
            .into_response(),
        Err(e) => {
            error!(error = % e, "心跳失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(HeartbeatResponse {
                    success: false,
                    error_message: e.to_string(),
                    config_changed: false,
                    latest_version: 0,
                }),
            )
                .into_response()
        }
    }
}
pub async fn list_nodes(State(state): State<NodeState>) -> Response {
    match state.repo.list_online().await {
        Ok(nodes) => (StatusCode::OK, Json(nodes)).into_response(),
        Err(e) => {
            error!(error = % e, "列出节点失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error" : "DATABASE_ERROR", "message" : e.to_string() })),
            )
                .into_response()
        }
    }
}
pub async fn discover_service(
    State(state): State<NodeState>,
    Path(service): Path<String>,
) -> Response {
    match state.repo.list_online().await {
        Ok(nodes) => {
            let filtered: Vec<RegisteredNode> = nodes
                .into_iter()
                .filter(|n| n.service_name == service)
                .collect();
            (StatusCode::OK, Json(filtered)).into_response()
        }
        Err(e) => {
            error!(error = % e, service = % service, "服务发现失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error" : "DATABASE_ERROR", "message" : e.to_string() })),
            )
                .into_response()
        }
    }
}
