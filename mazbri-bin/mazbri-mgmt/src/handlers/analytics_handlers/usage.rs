use super::super::AppState;
use super::super::types::AnalyticsParams;
use crate::services::analytics as service;
use axum::extract::{Path, Query, State};
pub async fn get_usage_analytics(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> axum::response::Response {
    service::get_usage_analytics(&state.db, &state.billing_db, &code, &params).await
}
