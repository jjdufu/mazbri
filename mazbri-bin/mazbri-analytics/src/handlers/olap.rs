use super::{AppState, HandlerError};
use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_analytics::clickhouse::olap::{OlapQuery, OlapRows, QueryWindow};
use serde::{Deserialize, Serialize};
use tracing::{error, warn};

#[derive(Debug, Serialize)]
struct OlapResponse {
    template: &'static str,
    rows: OlapRows,
}

fn bad_request(error: &str, message: String) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(HandlerError {
            error: error.to_string(),
            message,
        }),
    )
        .into_response()
}

/// `POST /api/v1/olap/query`：白名单模板查询；未知模板或非法参数返回 400。
pub async fn olap_query(State(state): State<AppState>, body: Bytes) -> Response {
    let query: OlapQuery = match serde_json::from_slice(&body) {
        Ok(query) => query,
        Err(e) => {
            warn!(error = %e, "OLAP 请求体无效");
            return bad_request("invalid_olap_query", e.to_string());
        }
    };
    if let Err(e) = query.validate() {
        return bad_request("invalid_olap_params", e.to_string());
    }
    match state.clickhouse_client.olap_query(&query).await {
        Ok(rows) => (
            StatusCode::OK,
            Json(OlapResponse {
                template: query.template_name(),
                rows,
            }),
        )
            .into_response(),
        Err(e) => {
            error!(template = query.template_name(), error = %e, "OLAP 查询失败");
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveProductsQuery {
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Serialize)]
pub struct ActiveProductsResponse {
    pub product_ids: Vec<String>,
}

/// `GET /api/v1/olap/active-products?start&end`：返回窗口内全量有调用去重 `product_id` 集合。
pub async fn get_active_products(
    State(state): State<AppState>,
    Query(query): Query<ActiveProductsQuery>,
) -> Response {
    let window = match QueryWindow::try_new(query.start, query.end) {
        Ok(window) => window,
        Err(e) => return bad_request("invalid_window", e.to_string()),
    };
    match state
        .clickhouse_client
        .fetch_active_product_ids(window.start, window.end)
        .await
    {
        Ok(product_ids) => (
            StatusCode::OK,
            Json(ActiveProductsResponse { product_ids }),
        )
            .into_response(),
        Err(e) => {
            error!(error = %e, "查询活跃产品集合失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(HandlerError {
                    error: "query_failed".to_string(),
                    message: "查询活跃产品集合失败".to_string(),
                }),
            )
                .into_response()
        }
    }
}
