use super::{AppState, HandlerError};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_analytics::greptimedb::{GreptimeDBClient, MetricQueryParams};
use tracing::error;
pub async fn get_metrics_overview(State(state): State<AppState>) -> Response {
    let Some(client) = metrics_client(&state) else {
        return greptimedb_unavailable();
    };
    match client.query_metrics_overview().await {
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
    if params.start_time >= params.end_time {
        return (
            StatusCode::BAD_REQUEST,
            Json(HandlerError {
                error: "invalid_time_range".to_string(),
                message: "start_time 必须小于 end_time".to_string(),
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
