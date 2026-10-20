use crate::handlers::{
    AppState, aggregate_request_logs, get_audit_analytics, get_metric_timeseries,
    get_metrics_catalog, get_metrics_overview, get_upstream_billing_summary, health_check,
    query_audit_logs, query_request_logs,
};
use axum::Router;
pub fn create_router(app_state: AppState) -> Router {
    Router::new()
        .route("/api/v1/audit/query", axum::routing::get(query_audit_logs))
        .route(
            "/api/v1/analytics/audit",
            axum::routing::get(get_audit_analytics),
        )
        .route(
            "/api/v1/request/query",
            axum::routing::get(query_request_logs),
        )
        .route(
            "/api/v1/analytics/requests",
            axum::routing::get(aggregate_request_logs),
        )
        .route(
            "/api/v1/analytics/upstream-billing",
            axum::routing::get(get_upstream_billing_summary),
        )
        .route(
            "/api/v1/metrics/overview",
            axum::routing::get(get_metrics_overview),
        )
        .route(
            "/api/v1/metrics/timeseries",
            axum::routing::get(get_metric_timeseries),
        )
        .route(
            "/api/v1/metrics/catalog",
            axum::routing::get(get_metrics_catalog),
        )
        .route("/health", axum::routing::get(health_check))
        .with_state(app_state)
}
