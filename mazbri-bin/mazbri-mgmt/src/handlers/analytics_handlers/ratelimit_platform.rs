use super::super::AppState;
use crate::services::analytics::{self, RateLimitAnalyticsParams};
use axum::extract::{Query, State};
pub async fn get_ratelimit_platform_stats(
    State(state): State<AppState>,
    Query(params): Query<RateLimitAnalyticsParams>,
) -> axum::response::Response {
    analytics::get_ratelimit_platform_stats(&state.ratelimit_db, &params).await
}
