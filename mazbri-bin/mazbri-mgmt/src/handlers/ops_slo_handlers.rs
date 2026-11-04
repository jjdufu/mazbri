//! 步骤 24：SLO API。

use super::AppState;
use crate::response::ApiResponse;
use crate::services::ops_slo as svc;
use axum::{extract::State, response::IntoResponse};
use serde_json::json;

/// `GET /stats/ops/slos`
pub async fn list_slos(State(state): State<AppState>) -> axum::response::Response {
    match svc::list_definitions(&state.admin_db).await {
        Ok(body) => ApiResponse::ok(body).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "list slos failed");
            ApiResponse::err(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("SLO 列表查询失败: {e}"),
            )
        }
    }
}

/// `GET /stats/ops/slos/status`
pub async fn list_slo_status(State(state): State<AppState>) -> axum::response::Response {
    let defs = match svc::list_definitions(&state.admin_db).await {
        Ok(v) => v,
        Err(e) => {
            return ApiResponse::err(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("SLO 定义查询失败: {e}"),
            );
        }
    };
    let status = match state.analytics_client.slo_status().await {
        Ok(v) => v,
        Err(e) => return e.into_response("SLO 状态查询"),
    };
    ApiResponse::ok(json!({
        "definitions": defs.get("items").cloned().unwrap_or(json!([])),
        "status": status.get("items").cloned().unwrap_or(json!([])),
    }))
    .into_response()
}
