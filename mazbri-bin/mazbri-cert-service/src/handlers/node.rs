use crate::config_center::ConfigRepository;
use crate::node_registry::{
    HeartbeatData, NodeRegistryRepository, RegisterNodeData, RegisteredNode,
};
use crate::token::scope::{ScopeContext, enforce_owner};
use axum::{
    Json,
    extract::{Extension, Path, State},
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
    pub cert_repo: Arc<crate::db::CertificateRepository>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
    pub revoked_serials_digest: String,
}
pub async fn register_node(
    State(state): State<NodeState>,
    Extension(scope): Extension<ScopeContext>,
    Json(req): Json<RegisterNodeData>,
) -> Response {
    if let Err(response) = enforce_owner(
        scope.owner_mode,
        scope.caller,
        &req.service_name,
        "/nodes/register",
    ) {
        return *response;
    }
    if let Err(message) = validate_node_registration(&req.ip, req.health_check_url.as_deref()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "INVALID_REQUEST", "message": message })),
        )
            .into_response();
    }
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
    Extension(scope): Extension<ScopeContext>,
    Json(req): Json<HeartbeatRequest>,
) -> Response {
    if let Err(response) = enforce_owner(
        scope.owner_mode,
        scope.caller,
        &req.service_name,
        "/nodes/heartbeat",
    ) {
        return *response;
    }
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
    let revoked_serials_digest = state
        .cert_repo
        .list_revoked_serials_hex()
        .await
        .map_or_else(|_| String::new(), |(digest, _)| digest);
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
                revoked_serials_digest,
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
                revoked_serials_digest,
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
                    revoked_serials_digest: String::new(),
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

fn validate_node_registration(ip: &str, health_check_url: Option<&str>) -> Result<(), String> {
    let registered_ip = ip
        .parse::<std::net::IpAddr>()
        .map_err(|_| "ip 必须为合法的 IPv4 或 IPv6 地址".to_string())?;
    let Some(raw_url) = health_check_url else {
        return Ok(());
    };
    if raw_url.is_empty() || raw_url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("health_check_url 格式非法".to_string());
    }
    if raw_url.contains('#') {
        return Err("health_check_url 禁止包含 fragment".to_string());
    }
    if raw_url.contains('?') {
        return Err("health_check_url 禁止包含查询参数".to_string());
    }
    if raw_url.contains('\\') {
        return Err("health_check_url 禁止包含反斜杠".to_string());
    }
    let uri = raw_url
        .parse::<axum::http::Uri>()
        .map_err(|_| "health_check_url 不是合法的 URL".to_string())?;
    let Some(scheme) = uri.scheme_str() else {
        return Err("health_check_url 缺少 scheme".to_string());
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return Err("health_check_url scheme 仅允许 http 或 https".to_string());
    }
    if !raw_url[scheme.len()..].starts_with("://") {
        return Err("health_check_url 必须使用标准 :// 格式".to_string());
    }
    let Some(authority) = uri.authority() else {
        return Err("health_check_url 缺少主机地址".to_string());
    };
    let auth_str = authority.as_str();
    if auth_str.contains('@') {
        return Err("health_check_url 禁止包含用户名或密码".to_string());
    }
    if auth_str.ends_with(':') || authority.port_u16() == Some(0) {
        return Err("health_check_url 端口非法".to_string());
    }
    match registered_ip {
        std::net::IpAddr::V4(expected_v4) => {
            let url_ip = authority
                .host()
                .parse::<std::net::Ipv4Addr>()
                .map_err(|_| "health_check_url 主机必须与注册 IP 一致".to_string())?;
            if url_ip != expected_v4 {
                return Err("health_check_url 主机必须与注册 IP 一致".to_string());
            }
        }
        std::net::IpAddr::V6(expected_v6) => {
            let bracketed = authority
                .host()
                .strip_prefix('[')
                .and_then(|host| host.strip_suffix(']'))
                .ok_or_else(|| "health_check_url IPv6 主机必须使用方括号包裹".to_string())?;
            let url_ip = bracketed
                .parse::<std::net::Ipv6Addr>()
                .map_err(|_| "health_check_url 主机必须与注册 IP 一致".to_string())?;
            if url_ip != expected_v6 {
                return Err("health_check_url 主机必须与注册 IP 一致".to_string());
            }
        }
    }
    if uri.query().is_some() {
        return Err("health_check_url 禁止包含查询参数".to_string());
    }
    if !matches!(uri.path(), "/health" | "/ready") {
        return Err("health_check_url 路径仅允许 /health 或 /ready".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_node_registration;

    #[test]
    fn accepts_valid_ip_and_matching_health_or_ready_url() {
        assert!(validate_node_registration("192.168.50.50", None).is_ok());
        assert!(
            validate_node_registration("192.168.50.50", Some("http://192.168.50.50:9000/health"))
                .is_ok()
        );
        assert!(
            validate_node_registration("192.168.50.50", Some("https://192.168.50.50:9000/ready"))
                .is_ok()
        );
        assert!(
            validate_node_registration("2001:db8::1", Some("http://[2001:db8:0:0::1]:9000/health"))
                .is_ok()
        );
    }

    #[test]
    fn rejects_invalid_ip_or_unsafe_health_check_urls() {
        for (ip, url) in [
            ("not-an-ip", None),
            ("192.168.50.50:9000", None),
            ("", None),
            ("192.168.50.50", Some("")),
            ("192.168.50.50", Some("ftp://192.168.50.50:9000/health")),
            (
                "192.168.50.50",
                Some("http://admin:secret@192.168.50.50:9000/health"),
            ),
            (
                "192.168.50.50",
                Some("http://admin@192.168.50.50:9000/health"),
            ),
            (
                "192.168.50.50",
                Some("http://192.168.50.50:9000/health?probe=1"),
            ),
            ("192.168.50.50", Some("http://192.168.50.50:9000/health?")),
            (
                "192.168.50.50",
                Some("http://192.168.50.50:9000/health#frag"),
            ),
            ("192.168.50.50", Some("http://192.168.50.51:9000/health")),
            ("192.168.50.50", Some("http://localhost:9000/health")),
            (
                "127.0.0.1",
                Some("http://[::ffff:127.0.0.1]:9000/health"),
            ),
            ("192.168.50.50", Some("http://192.168.50.50:9000/metrics")),
            (
                "192.168.50.50",
                Some("http://192.168.50.50:9000/admin/../health"),
            ),
            ("192.168.50.50", Some("http://192.168.50.50:9000//health")),
            ("192.168.50.50", Some("http://192.168.50.50:0/health")),
        ] {
            assert!(
                validate_node_registration(ip, url).is_err(),
                "expected rejection for ip={ip}, url={url:?}"
            );
        }
    }
}
