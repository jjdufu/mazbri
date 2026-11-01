//! 步骤 21：服务拓扑 API。
use super::{AppState, HandlerError};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use tracing::warn;

#[derive(Debug, Deserialize)]
pub struct TopologyQuery {
    /// Unix 秒；默认 now-1h。
    pub start: Option<i64>,
    /// Unix 秒；默认 now。
    pub end: Option<i64>,
}

/// `GET /api/v1/metrics/topology`
pub async fn get_topology(
    State(state): State<AppState>,
    Query(q): Query<TopologyQuery>,
) -> Response {
    let now = chrono::Utc::now().timestamp();
    let end = q.end.unwrap_or(now).min(now + 60);
    let start = q.start.unwrap_or(end - 3600);
    if start >= end {
        return (
            StatusCode::BAD_REQUEST,
            Json(HandlerError {
                error: "invalid_time_range".into(),
                message: "start 必须小于 end".into(),
            }),
        )
            .into_response();
    }
    // 最长 24h，与 traces/dependencies 一致
    let start = start.max(end - 86_400);
    match mazbri_analytics::topology::query_topology(
        state.greptimedb_client.clone(),
        state.clickhouse_client.clone(),
        start,
        end,
    )
    .await
    {
        Ok(graph) => (StatusCode::OK, Json(graph)).into_response(),
        Err(e) if e == "greptimedb_unavailable" => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HandlerError {
                error: "service_unavailable".into(),
                message: "GreptimeDB 服务不可用".into(),
            }),
        )
            .into_response(),
        Err(e) => {
            warn!(error = %e, "拓扑查询失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(HandlerError {
                    error: "query_failed".into(),
                    message: "查询失败".into(),
                }),
            )
                .into_response()
        }
    }
}
