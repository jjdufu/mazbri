//! 步骤 22：变更时间线只读接口。

use super::AppState;
use crate::response::ApiResponse;
use crate::services::ops_changes as svc;
use axum::{
    extract::{Query, State},
    response::IntoResponse,
};
use chrono::{Duration, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangesQuery {
    /// Unix 秒；缺省 = now-7d。
    pub start: Option<i64>,
    /// Unix 秒；缺省 = now。
    pub end: Option<i64>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

const fn default_limit() -> i64 {
    100
}

/// `GET /stats/ops/changes`
pub async fn get_ops_changes(
    State(state): State<AppState>,
    Query(query): Query<ChangesQuery>,
) -> axum::response::Response {
    let end = query
        .end
        .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
        .unwrap_or_else(Utc::now);
    let start = query
        .start
        .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
        .unwrap_or_else(|| end - Duration::days(7));
    match svc::list_changes(&state.admin_db, start, end, query.limit).await {
        Ok(body) => ApiResponse::ok(body).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "ops changes query failed");
            ApiResponse::err(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("变更时间线查询失败: {e}"),
            )
        }
    }
}
