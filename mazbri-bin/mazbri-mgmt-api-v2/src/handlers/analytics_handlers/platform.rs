use super::super::AppState;
use super::super::types::PlatformStatsParams;
use crate::services::analytics as service;
use axum::extract::{Query, State};
pub async fn get_platform_stats(
    State(state): State<AppState>,
    Query(params): Query<PlatformStatsParams>,
) -> axum::response::Response {
    service::get_platform_stats(&state.db, &state.analytics_client, &params).await
}
