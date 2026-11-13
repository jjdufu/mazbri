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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZeroCallProductsQuery {
    pub start: i64,
    pub end: i64,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_zero_call_page_size")]
    pub page_size: u32,
    #[serde(default)]
    pub q: Option<String>,
}

const fn default_page() -> u32 {
    1
}

const fn default_zero_call_page_size() -> u32 {
    10
}

/// `GET /stats/ops/products/zero-calls?start&end&page&page_size&q`：真服务端分页零调用产品列表。
pub async fn list_zero_call_products(
    State(state): State<AppState>,
    Query(query): Query<ZeroCallProductsQuery>,
) -> axum::response::Response {
    let window = match resolve_timeseries_window(query.start, query.end) {
        Ok(w) => w,
        Err(message) => return ApiResponse::err(StatusCode::BAD_REQUEST, message),
    };
    let active_ids = match state
        .analytics_client
        .fetch_active_product_ids(window.start_unix, window.end_unix)
        .await
    {
        Ok(ids) => ids,
        Err(e) => {
            tracing::error!(error = %e, "fetch active product ids from analytics failed");
            return ApiResponse::err(
                StatusCode::BAD_GATEWAY,
                format!("查询活跃产品集合失败: {e}"),
            );
        }
    };
    match svc::list_zero_call_products_page(
        &state.db,
        &active_ids,
        query.page,
        query.page_size,
        query.q.as_deref(),
    )
    .await
    {
        Ok(page) => ApiResponse::ok(page).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "list zero-call products failed");
            ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("零调用产品分页查询失败: {e}"),
            )
        }
    }
}
