use super::super::AppState;
use crate::services::analytics::{self, RateLimitAnalyticsParams};
use axum::extract::{Path, Query, State};
pub async fn get_ratelimit_analytics(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Query(params): Query<RateLimitAnalyticsParams>,
) -> axum::response::Response {
    analytics::get_ratelimit_analytics(&state.db, &state.ratelimit_db, &code, &params).await
}
