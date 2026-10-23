use super::{AppState, HandlerError, parse_time_string};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use mazbri_analytics::greptimedb::{RequestLogAggregationParams, TimeBucket};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{error, info};
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestAnalyticsQuery {
    pub start_time: String,
    pub end_time: String,
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
    /// Access-log producer service filter (e.g. `mazbri-gateway`). Avoids double-counting
    /// gateway + bypass rows that share the same customer `request_id`.
    #[serde(default)]
    pub service: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RequestAnalyticsResponse {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_requests: u64,
    pub avg_duration_ms: f64,
    pub avg_upstream_service_time_ms: f64,
    pub avg_dependency_wait_ms: f64,
    pub avg_internal_ms: f64,
    pub avg_post_response_ms: f64,
    pub by_route: HashMap<String, u64>,
    pub by_status: HashMap<u16, u64>,
    pub by_time: Vec<TimeBucket>,
}
impl From<mazbri_analytics::greptimedb::RequestLogAggregation> for RequestAnalyticsResponse {
    fn from(agg: mazbri_analytics::greptimedb::RequestLogAggregation) -> Self {
        Self {
            period_start: agg.period_start,
            period_end: agg.period_end,
            total_requests: agg.total_requests,
            avg_duration_ms: agg.avg_duration_ms,
            avg_upstream_service_time_ms: agg.avg_upstream_service_time_ms,
            avg_dependency_wait_ms: agg.avg_dependency_wait_ms,
            avg_internal_ms: agg.avg_internal_ms,
            avg_post_response_ms: agg.avg_post_response_ms,
            by_route: agg.by_route,
            by_status: agg.by_status,
            by_time: agg.by_time,
        }
    }
}
pub async fn aggregate_request_logs(
    State(state): State<AppState>,
    Query(req): Query<RequestAnalyticsQuery>,
) -> Response {
    info!(
        start_time = %req.start_time,
        end_time = %req.end_time,
        group_by = ?req.group_by,
        tenant_id = ?req.tenant_id,
        service = ?req.service,
        "查询请求日志分析"
    );
    let client = &state.clickhouse_client;
    let Some(start_time) = parse_time_string(&req.start_time) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(HandlerError {
                error: "invalid_start_time".to_string(),
                message: "start_time 格式无效，请使用 Unix 时间戳或 ISO 8601 格式".to_string(),
            }),
        )
            .into_response();
    };
    let Some(end_time) = parse_time_string(&req.end_time) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(HandlerError {
                error: "invalid_end_time".to_string(),
                message: "end_time 格式无效，请使用 Unix 时间戳或 ISO 8601 格式".to_string(),
            }),
        )
            .into_response();
    };
    if start_time >= end_time {
        return (
            StatusCode::BAD_REQUEST,
            Json(HandlerError {
                error: "invalid_time_range".to_string(),
                message: "start_time 必须小于 end_time".to_string(),
            }),
        )
            .into_response();
    }
    let params = RequestLogAggregationParams {
        start_time,
        end_time,
        group_by_route: true,
        group_by_time: req.group_by,
        tenant_id: req.tenant_id,
        service: req
            .service
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    };
    match client.aggregate_request_logs(&params).await {
        Ok(aggregation) => {
            let response = RequestAnalyticsResponse::from(aggregation);
            info!(
                total_requests = response.total_requests,
                avg_duration_ms = response.avg_duration_ms,
                avg_upstream_service_time_ms = response.avg_upstream_service_time_ms,
                avg_dependency_wait_ms = response.avg_dependency_wait_ms,
                routes = response.by_route.len(),
                statuses = response.by_status.len(),
                time_buckets = response.by_time.len(),
                "分析完成"
            );
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("请求日志分析查询失败: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(HandlerError {
                    error: "query_failed".to_string(),
                    message: "查询失败".to_string(),
                }),
            )
                .into_response()
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamSummaryQuery {
    pub start_time: String,
    pub end_time: String,
    pub supplier_code: Option<String>,
    pub channel_code: Option<String>,
    pub supplier_id: Option<String>,
    pub channel_id: Option<String>,
}

pub async fn get_upstream_billing_summary(
    State(state): State<AppState>,
    Query(req): Query<UpstreamSummaryQuery>,
) -> Response {
    let client = &state.clickhouse_client;
    match client
        .fetch_upstream_usage_summary(
            &req.start_time,
            &req.end_time,
            req.supplier_code.as_deref(),
            req.channel_code.as_deref(),
            req.supplier_id.as_deref(),
            req.channel_id.as_deref(),
        )
        .await
    {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => {
            error!("Upstream usage summary query failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(HandlerError {
                    error: "query_failed".to_string(),
                    message: "Query upstream summary failed".to_string(),
                }),
            )
                .into_response()
        }
    }
}
