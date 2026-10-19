use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_analytics::greptimedb::GreptimeDBClient;
use mazbri_analytics::clickhouse::ClickHouseClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub greptimedb_connected: bool,
    pub clickhouse_connected: bool,
    pub message: String,
}
#[derive(Clone)]
pub struct AppState {
    pub greptimedb_client: Option<Arc<GreptimeDBClient>>,
    pub clickhouse_client: Arc<ClickHouseClient>,
}
pub async fn health_check(State(state): State<AppState>) -> Response {
    let clickhouse_connected = match state.clickhouse_client.test_connection().await {
        Ok(_) => {
            info!("ClickHouse 连接正常");
            true
        }
        Err(e) => {
            error!("ClickHouse 连接失败: {}", e);
            false
        }
    };
    let greptimedb_connected = if let Some(client) = &state.greptimedb_client {
        match client.test_connection().await {
            Ok(_) => {
                info!("GreptimeDB 连接正常");
                true
            }
            Err(e) => {
                error!("GreptimeDB 连接失败: {}", e);
                false
            }
        }
    } else {
        info!("GreptimeDB 客户端未配置");
        false
    };
    let status = if clickhouse_connected && greptimedb_connected {
        "ok"
    } else if clickhouse_connected {
        "degraded"
    } else {
        "unavailable"
    };
    let message = if clickhouse_connected && greptimedb_connected {
        "服务正常"
    } else if clickhouse_connected {
        "GreptimeDB 未连接，指标监控功能降级"
    } else if greptimedb_connected {
        "ClickHouse 未连接，日志分析功能不可用"
    } else {
        "所有数据库服务未连接"
    };
    let response = HealthResponse {
        status: status.to_string(),
        greptimedb_connected,
        clickhouse_connected,
        message: message.to_string(),
    };
    (StatusCode::OK, Json(response)).into_response()
}
