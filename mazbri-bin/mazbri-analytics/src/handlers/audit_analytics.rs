use super::{
    AppState, AuditAnalyticsRequest, AuditAnalyticsResponse, HandlerError, parse_time_string,
};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_analytics::greptimedb::AuditAnalyticsParams;
use std::collections::HashMap;
use tracing::{error, info};
pub async fn get_audit_analytics(
    State(state): State<AppState>,
    Query(req): Query<AuditAnalyticsRequest>,
) -> Response {
    let client = &state.clickhouse_client;
    if let Some(admin_id) = req.admin_id {
        if admin_id <= 0 {
            return (
                StatusCode::BAD_REQUEST,
                Json(HandlerError {
                    error: "invalid_admin_id".to_string(),
                    message: "admin_id must be a positive integer".to_string(),
                }),
            )
                .into_response();
        }
    }
    let group_by = match req.group_by.as_str() {
        "day" | "hour" => req.group_by.clone(),
        _ => {
            info!(
                invalid_group_by = % req.group_by,
                "无效的 group_by 参数，使用默认值 'day'"
            );
            "day".to_string()
        }
    };
    let start_time = match req.start_time.as_ref() {
        Some(s) => match parse_time_string(s) {
            Some(t) => Some(t),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(HandlerError {
                        error: "invalid_start_time".to_string(),
                        message: "start_time 格式无效，请使用 Unix 时间戳或 ISO 8601 格式"
                            .to_string(),
                    }),
                )
                    .into_response();
            }
        },
        None => None,
    };
    let end_time = match req.end_time.as_ref() {
        Some(s) => match parse_time_string(s) {
            Some(t) => Some(t),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(HandlerError {
                        error: "invalid_end_time".to_string(),
                        message: "end_time 格式无效，请使用 Unix 时间戳或 ISO 8601 格式"
                            .to_string(),
                    }),
                )
                    .into_response();
            }
        },
        None => None,
    };
    let params = AuditAnalyticsParams {
        start_time,
        end_time,
        group_by,
        admin_id: req.admin_id,
    };
    let results = match client.query_audit_analytics(&params).await {
        Ok(results) => results,
        Err(e) => {
            error!("审计日志分析查询失败: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(HandlerError {
                    error: "query_failed".to_string(),
                    message: "查询失败".to_string(),
                }),
            )
                .into_response();
        }
    };
    let response = build_analytics_response(results);
    (StatusCode::OK, Json(response)).into_response()
}
fn build_analytics_response(
    results: Vec<mazbri_analytics::greptimedb::AuditAnalyticsResult>,
) -> AuditAnalyticsResponse {
    let mut timeline_map: HashMap<String, AuditAnalyticsTimelineBuilder> = HashMap::new();
    let mut total_actions: u64 = 0;
    let mut by_action: HashMap<String, u64> = HashMap::new();
    let mut by_resource: HashMap<String, u64> = HashMap::new();
    for result in results {
        total_actions += result.count;
        *by_action.entry(result.action.clone()).or_insert(0) += result.count;
        *by_resource.entry(result.resource_type.clone()).or_insert(0) += result.count;
        let builder = timeline_map.entry(result.period.clone()).or_default();
        builder.total_actions += result.count;
        *builder.by_action.entry(result.action.clone()).or_insert(0) += result.count;
        *builder
            .by_resource
            .entry(result.resource_type.clone())
            .or_insert(0) += result.count;
    }
    let mut timeline: Vec<_> = timeline_map
        .into_iter()
        .map(|(period, builder)| super::AuditAnalyticsTimelineItem {
            period,
            total_actions: builder.total_actions,
            by_action: builder.by_action,
            by_resource: builder.by_resource,
        })
        .collect();
    timeline.sort_by(|a, b| b.period.cmp(&a.period));
    let (period_start, period_end) = if timeline.is_empty() {
        (String::new(), String::new())
    } else {
        let last = &timeline[timeline.len() - 1];
        let first = &timeline[0];
        (last.period.clone(), first.period.clone())
    };
    AuditAnalyticsResponse {
        period_start,
        period_end,
        total_actions,
        by_action,
        by_resource,
        timeline,
    }
}
#[derive(Default)]
struct AuditAnalyticsTimelineBuilder {
    total_actions: u64,
    by_action: HashMap<String, u64>,
    by_resource: HashMap<String, u64>,
}
