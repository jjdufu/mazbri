use super::super::analytics_handlers;
use super::super::ops_changes_handlers;
use super::super::ops_anomalies_handlers;
use super::super::ops_slo_handlers;
use super::super::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};

fn metrics_and_ops_routes() -> Router<AppState> {
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
            "/stats/metrics/services/health",
            get(analytics_handlers::get_services_health),
        )
        .route(
            "/stats/metrics/topology",
            get(analytics_handlers::get_metrics_topology),
        )
        .route(
            "/stats/olap/query",
            post(analytics_handlers::post_olap_query),
        )
        .route(
            "/stats/ops/changes",
            get(ops_changes_handlers::get_ops_changes),
        )
        .route(
            "/stats/ops/anomalies",
            get(ops_anomalies_handlers::list_anomalies),
        )
        .route("/stats/ops/slos", get(ops_slo_handlers::list_slos))
        .route(
            "/stats/ops/slos/status",
            get(ops_slo_handlers::list_slo_status),
        )
        .route(
            "/stats/ops/products/zero-calls",
            post(ops_anomalies_handlers::zero_call_products),
        )
}

fn calls_and_traces_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/stats/calls/customer",
            get(analytics_handlers::get_customer_call_logs),
        )
        .route(
            "/stats/calls/upstream",
            get(analytics_handlers::get_upstream_call_logs),
        )
        .route(
            "/stats/calls/customer/histogram",
            get(analytics_handlers::get_customer_call_histogram),
        )
        .route(
            "/stats/calls/upstream/histogram",
            get(analytics_handlers::get_upstream_call_histogram),
        )
        .route(
            "/stats/calls/stages",
            get(analytics_handlers::get_call_stages),
        )
        .route(
            "/stats/traces/services",
            get(analytics_handlers::list_trace_services),
        )
        .route(
            "/stats/traces/search",
            get(analytics_handlers::search_traces),
        )
        .route(
            "/stats/traces/dependencies",
            get(analytics_handlers::get_trace_dependencies),
        )
        .route(
            "/stats/traces/billing",
            get(analytics_handlers::get_trace_billing),
        )
        .route(
            "/stats/traces/{trace_id}",
            get(analytics_handlers::get_trace),
        )
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(metrics_and_ops_routes())
        .merge(calls_and_traces_routes())
}
