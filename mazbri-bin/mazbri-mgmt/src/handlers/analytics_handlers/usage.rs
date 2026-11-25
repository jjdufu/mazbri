use super::super::AppState;
use super::super::types::AnalyticsParams;
use crate::middleware::auth::AdminClaims;
use crate::services::analytics as service;
use axum::extract::{Extension, Path, Query, State};
pub async fn get_usage_analytics(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(code): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> axum::response::Response {
    service::get_usage_analytics(&state.db, &state.billing_db, &code, &claims, &params).await
}
