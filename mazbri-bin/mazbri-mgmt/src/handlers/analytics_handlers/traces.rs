//! Trace 页面接口。
use super::super::AppState;
use crate::services::traces::{self, TraceBillingParams, TraceSearchParams, TraceWindowParams};
use axum::extract::{Path, Query, State};

/// `GET /stats/traces/search`
pub async fn search_traces(
    State(state): State<AppState>,
    Query(params): Query<TraceSearchParams>,
) -> axum::response::Response {
    traces::search(&state.analytics_client, &params).await
}

/// `GET /stats/traces/dependencies`
pub async fn get_trace_dependencies(
    State(state): State<AppState>,
    Query(params): Query<TraceWindowParams>,
) -> axum::response::Response {
    traces::dependencies(&state.analytics_client, &params).await
}

/// `GET /stats/traces/billing`：按 `request_id` 查计费流水。
pub async fn get_trace_billing(
    State(state): State<AppState>,
    Query(params): Query<TraceBillingParams>,
) -> axum::response::Response {
    traces::billing(&state.billing_db, &params).await
}

/// `GET /stats/traces/{trace_id}`
pub async fn get_trace(
    State(state): State<AppState>,
    Path(trace_id): Path<String>,
    Query(params): Query<TraceWindowParams>,
) -> axum::response::Response {
    traces::detail(&state.analytics_client, &trace_id, &params).await
}
