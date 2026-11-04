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
/// Overview 查询参数：默认跟时间窗；`mode=instant`（或无 start/end）为近瞬时「此刻」。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverviewQuery {
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub end: Option<i64>,
    /// `instant` = 此刻；其余或缺省且带 start/end = range。
    #[serde(default)]
    pub mode: Option<String>,
}

/// 指标概览：默认 `window.semantics=range`（跟 start/end）；`mode=instant` 为近瞬时。
/// 无样本时率类字段为 null。
pub async fn get_metrics_overview(
    State(state): State<AppState>,
    Query(q): Query<OverviewQuery>,
) -> Response {
    use mazbri_analytics::greptimedb::OverviewKpiMode;
    let Some(client) = metrics_client(&state) else {
        return greptimedb_unavailable();
    };
    let mode = match q.mode.as_deref() {
        Some("instant") => OverviewKpiMode::Instant,
        _ => match (q.start, q.end) {
            (Some(start), Some(end)) if start < end => OverviewKpiMode::Range { start, end },
            (None, None) => OverviewKpiMode::Instant,
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(HandlerError {
                        error: "invalid_time_range".to_string(),
                        message: "start/end 无效：须同时提供且 start < end，或使用 mode=instant"
                            .to_string(),
                    }),
                )
                    .into_response();
            }
        },
    };
    match client.query_metrics_overview(mode).await {
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
