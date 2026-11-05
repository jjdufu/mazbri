use super::{AppState, HandlerError};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_analytics::greptimedb::{GreptimeDBClient, MetricQueryParams};
use serde::Deserialize;
use tracing::error;
/// Overview 查询参数：`start` / `end`（Unix 秒）必填，窗长 60s–31d，不截断。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverviewQuery {
    pub start: i64,
    pub end: i64,
}

/// 指标概览：`traffic` 跟 `[start, end]`；`realtime` / `components` 为当前时刻。
pub async fn get_metrics_overview(
    State(state): State<AppState>,
    Query(q): Query<OverviewQuery>,
) -> Response {
    use mazbri_analytics::greptimedb::{OVERVIEW_MAX_SECS, OVERVIEW_MIN_SECS};
    let Some(client) = metrics_client(&state) else {
        return greptimedb_unavailable();
    };
    let span = u64::try_from(q.end.saturating_sub(q.start)).unwrap_or(0);
    if !(OVERVIEW_MIN_SECS..=OVERVIEW_MAX_SECS).contains(&span) {
        return (
            StatusCode::BAD_REQUEST,
            Json(HandlerError {
                error: "invalid_time_range".to_string(),
                message: format!(
                    "start/end 无效：窗长须在 {OVERVIEW_MIN_SECS}s 到 {OVERVIEW_MAX_SECS}s 之间"
                ),
            }),
        )
            .into_response();
    }
    match client.query_metrics_overview(q.start, q.end).await {
        Ok(overview) => (StatusCode::OK, Json(overview)).into_response(),
        Err(e) => {
            error!("指标概览查询失败: {}", e);
            query_failed()
        }
    }
}
pub async fn get_metric_timeseries(
    State(state): State<AppState>,
    Query(params): Query<MetricQueryParams>,
) -> Response {
    let Some(client) = metrics_client(&state) else {
        return greptimedb_unavailable();
    };
    if params.start >= params.end {
        return (
            StatusCode::BAD_REQUEST,
            Json(HandlerError {
                error: "invalid_time_range".to_string(),
                message: "start 必须小于 end".to_string(),
            }),
        )
            .into_response();
    }
    match client.query_metric_series(&params).await {
        Ok(series) => (StatusCode::OK, Json(series)).into_response(),
        Err(e) => {
            error!("指标时序查询失败: {}", e);
            query_failed()
        }
    }
}
pub async fn get_metrics_catalog(State(state): State<AppState>) -> Response {
    let Some(client) = metrics_client(&state) else {
        return greptimedb_unavailable();
    };
    (
        StatusCode::OK,
        Json(serde_json::json!(
            { "services" : client.metric_services(), "metrics" :
            client.metric_catalog(), }
        )),
    )
        .into_response()
}
fn metrics_client(state: &AppState) -> Option<&GreptimeDBClient> {
    state.greptimedb_client.as_deref()
}
fn greptimedb_unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(HandlerError {
            error: "service_unavailable".to_string(),
            message: "GreptimeDB 服务不可用".to_string(),
        }),
    )
        .into_response()
}
fn query_failed() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(HandlerError {
            error: "query_failed".to_string(),
            message: "查询失败".to_string(),
        }),
    )
        .into_response()
}
pub async fn get_services_health(State(state): State<AppState>) -> Response {
    let Some(client) = metrics_client(&state) else {
        return greptimedb_unavailable();
    };
    match client.query_services_health().await {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => {
            error!("服务健康矩阵查询失败: {}", e);
            query_failed()
        }
    }
}
