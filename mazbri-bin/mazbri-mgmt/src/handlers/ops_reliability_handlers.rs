//! 步骤 26：压测 / 异常 / 容量 / 零调用产品 API。

use super::AppState;
use crate::response::ApiResponse;
use crate::services::ops_reliability as svc;
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

const fn default_limit() -> i64 {
    50
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnomalyQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    pub kind: Option<String>,
}

/// `GET /stats/ops/loadtests`
pub async fn list_loadtests(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> axum::response::Response {
    match svc::list_loadtests(&state.admin_db, q.limit).await {
        Ok(body) => ApiResponse::ok(body).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "list loadtests failed");
            ApiResponse::err(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("压测基线查询失败: {e}"),
            )
        }
    }
}

/// `POST /stats/ops/loadtests`
pub async fn create_loadtest(
    State(state): State<AppState>,
    Json(req): Json<svc::CreateLoadtestRun>,
) -> axum::response::Response {
    match svc::create_loadtest(&state.admin_db, &req).await {
        Ok(body) => ApiResponse::ok(body).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "create loadtest failed");
            ApiResponse::err(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("录入压测失败: {e}"),
            )
        }
    }
}

/// `GET /stats/ops/anomalies`
pub async fn list_anomalies(
    State(state): State<AppState>,
    Query(q): Query<AnomalyQuery>,
) -> axum::response::Response {
    match svc::list_anomalies(&state.admin_db, q.limit, q.kind.as_deref()).await {
        Ok(body) => ApiResponse::ok(body).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "list anomalies failed");
            ApiResponse::err(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("异常事件查询失败: {e}"),
            )
        }
    }
}

/// `GET /stats/ops/capacity`
pub async fn get_capacity(
    State(state): State<AppState>,
) -> axum::response::Response {
    match svc::latest_capacity(&state.admin_db).await {
        Ok(body) => ApiResponse::ok(body).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "capacity snapshot failed");
            ApiResponse::err(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("容量预测查询失败: {e}"),
            )
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZeroCallBody {
    /// 近 30 天有调用的 `product_id` 列表（前端或代理从 OLAP 取得）。
    #[serde(default)]
    pub active_product_ids: Vec<String>,
}

/// `POST /stats/ops/products/zero-calls`：平台产品 − 活跃集合。
pub async fn zero_call_products(
    State(state): State<AppState>,
    Json(body): Json<ZeroCallBody>,
) -> axum::response::Response {
    let active: std::collections::HashSet<_> = body
        .active_product_ids
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    match svc::list_platform_products(&state.db).await {
        Ok(all) => {
            let zero: Vec<svc::ZeroCallProduct> = all
                .into_iter()
                .filter(|(id, _)| !active.contains(id))
                .map(|(product_id, name)| svc::ZeroCallProduct { product_id, name })
                .collect();
            ApiResponse::ok(zero).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "zero-call products failed");
            ApiResponse::err(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("零调用产品查询失败: {e}"),
            )
        }
    }
}
