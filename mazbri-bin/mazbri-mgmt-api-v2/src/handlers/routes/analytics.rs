use super::super::analytics_handlers;
use super::super::state::AppState;
use axum::{Router, routing::get};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/tenants/{code}/analytics",
            get(analytics_handlers::get_usage_analytics),
        )
        .route(
            "/tenants/{code}/analytics/ratelimit",
            get(analytics_handlers::get_ratelimit_analytics),
        )
        .route(
            "/stats/platform",
            get(analytics_handlers::get_platform_stats),
        )
        .route(
            "/stats/ratelimit",
            get(analytics_handlers::get_ratelimit_platform_stats),
        )
        .route(
            "/stats/metrics/overview",
            get(analytics_handlers::get_metrics_overview),
        )
        .route(
            "/stats/metrics/timeseries",
            get(analytics_handlers::get_metric_timeseries),
        )
        .route(
            "/stats/metrics/catalog",
            get(analytics_handlers::get_metrics_catalog),
        )
        .route(
            "/stats/calls/customer",
            get(analytics_handlers::get_customer_call_logs),
        )
        .route(
            "/stats/calls/upstream",
            get(analytics_handlers::get_upstream_call_logs),
        )
}
