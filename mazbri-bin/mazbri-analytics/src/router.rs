use crate::handlers::{
    AppState, aggregate_request_logs, get_active_products, get_metric_timeseries,
    get_metrics_catalog, get_metrics_overview, get_services_health, get_slo_status, get_topology,
    get_trace, get_upstream_billing_summary, get_upstream_hourly_billing, health_check,
    list_trace_services, olap_query, query_request_logs, request_histogram, request_stages,
    search_traces, trace_dependencies,
};
use axum::Router;
pub fn create_router(app_state: AppState) -> Router {
    Router::new()
        .route(
            "/api/v1/request/query",
            axum::routing::get(query_request_logs),
        )
        .route(
            "/api/v1/request/histogram",
            axum::routing::get(request_histogram),
        )
        .route("/api/v1/request/stages", axum::routing::get(request_stages))
        .route(
            "/api/v1/analytics/requests",
            axum::routing::get(aggregate_request_logs),
        )
        .route(
            "/api/v1/analytics/upstream-billing",
            axum::routing::get(get_upstream_billing_summary),
        )
        .route(
            "/api/v1/analytics/upstream-billing/hourly",
            axum::routing::get(get_upstream_hourly_billing),
        )
        .route(
            "/api/v1/metrics/overview",
            axum::routing::get(get_metrics_overview),
        )
        .route(
            "/api/v1/slo/status",
            axum::routing::get(get_slo_status),
        )
        .route(
            "/api/v1/metrics/timeseries",
            axum::routing::get(get_metric_timeseries),
        )
        .route(
            "/api/v1/metrics/catalog",
            axum::routing::get(get_metrics_catalog),
        )
        .route(
            "/api/v1/metrics/services/health",
            axum::routing::get(get_services_health),
        )
        .route(
            "/api/v1/metrics/topology",
            axum::routing::get(get_topology),
        )
        .route("/api/v1/olap/query", axum::routing::post(olap_query))
        .route(
            "/api/v1/olap/active-products",
            axum::routing::get(get_active_products),
        )
        .route("/api/v1/traces/search", axum::routing::get(search_traces))
        .route(
            "/api/v1/traces/services",
            axum::routing::get(list_trace_services),
        )
        .route(
            "/api/v1/traces/dependencies",
            axum::routing::get(trace_dependencies),
        )
        .route("/api/v1/traces/{trace_id}", axum::routing::get(get_trace))
        .route("/health", axum::routing::get(health_check))
        .with_state(app_state)
}
