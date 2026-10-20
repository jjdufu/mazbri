use super::super::AppState;
use crate::services::analytics::{self, CallMonitorParams, MetricsTimeseriesParams};
use axum::extract::{Query, State};
pub async fn get_metrics_overview(State(state): State<AppState>) -> axum::response::Response {
    analytics::get_metrics_overview(&state.analytics_client).await
}
pub async fn get_metrics_catalog(State(state): State<AppState>) -> axum::response::Response {
    analytics::get_metrics_catalog(&state.analytics_client).await
}
pub async fn get_metric_timeseries(
    State(state): State<AppState>,
    Query(params): Query<MetricsTimeseriesParams>,
) -> axum::response::Response {
    analytics::get_metric_timeseries(&state.analytics_client, &params).await
}
pub async fn get_customer_call_logs(
    State(state): State<AppState>,
    Query(params): Query<CallMonitorParams>,
) -> axum::response::Response {
    analytics::get_call_monitor_logs(&state.db, &state.analytics_client, "customer", &params).await
}
pub async fn get_upstream_call_logs(
    State(state): State<AppState>,
    Query(params): Query<CallMonitorParams>,
) -> axum::response::Response {
    analytics::get_call_monitor_logs(&state.db, &state.analytics_client, "upstream", &params).await
}
