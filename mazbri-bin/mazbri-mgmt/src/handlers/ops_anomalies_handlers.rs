//! 运维异常事件与零调用产品 API（原可靠性页压测/容量已删除）。

use super::AppState;
use crate::response::ApiResponse;
use crate::services::ops_anomalies as svc;
use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnomalyQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    pub kind: Option<String>,
}

const fn default_limit() -> i64 {
    50
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
