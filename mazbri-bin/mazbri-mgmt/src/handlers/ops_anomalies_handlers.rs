//! 运维异常事件与平台产品名录 API。

use super::AppState;
use crate::response::ApiResponse;
use crate::services::analytics_window::resolve_timeseries_window;
use crate::services::ops_anomalies as svc;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnomalyQuery {
    /// 时间窗起点（Unix 秒）。
    pub start: i64,
    /// 时间窗终点（Unix 秒）。
    pub end: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

const fn default_limit() -> i64 {
    50
}

/// `GET /stats/ops/anomalies?start&end&limit`：时间窗内最近异常（60s–31d）。
pub async fn list_anomalies(
    State(state): State<AppState>,
    Query(q): Query<AnomalyQuery>,
) -> axum::response::Response {
    let window = match resolve_timeseries_window(q.start, q.end) {
        Ok(w) => w,
        Err(message) => return ApiResponse::err(StatusCode::BAD_REQUEST, message),
    };
    let (Some(start), Some(end)) = (
        DateTime::<Utc>::from_timestamp(window.start_unix, 0),
        DateTime::<Utc>::from_timestamp(window.end_unix, 0),
    ) else {
        return ApiResponse::err(StatusCode::BAD_REQUEST, "时间戳超出范围");
    };
    match svc::list_anomalies(&state.admin_db, start, end, q.limit).await {
        Ok(body) => ApiResponse::ok(body).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "list anomalies failed");
            ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("异常事件查询失败: {e}"),
            )
        }
    }
}

/// `GET /stats/ops/products/names`：已启用产品的 id → 名称（TopN 显示名、零调用差集）。
pub async fn list_product_names(State(state): State<AppState>) -> axum::response::Response {
    match svc::list_platform_products(&state.db).await {
        Ok(rows) => ApiResponse::ok(rows).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "list product names failed");
            ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("产品名录查询失败: {e}"),
            )
        }
    }
}
