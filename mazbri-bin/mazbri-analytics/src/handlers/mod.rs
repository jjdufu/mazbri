mod call_search;
mod health;
mod metrics;
mod olap;
mod request;
mod request_analytics;
mod time;
mod traces;
#[derive(Debug, serde::Serialize)]
pub(crate) struct HandlerError {
    pub error: String,
    pub message: String,
}
pub use call_search::{request_histogram, request_stages};
pub use health::AppState;
pub use health::health_check;
pub use metrics::{
    get_metric_timeseries, get_metrics_catalog, get_metrics_overview, get_services_health,
};
pub use olap::olap_query;
pub use request::query_request_logs;
pub use request_analytics::{aggregate_request_logs, get_upstream_billing_summary};
pub use time::parse_time_string;
pub use traces::{get_trace, search_traces, trace_dependencies};
