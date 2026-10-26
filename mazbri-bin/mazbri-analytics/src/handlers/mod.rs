mod health;
mod metrics;
mod request;
mod request_analytics;
mod time;
#[derive(Debug, serde::Serialize)]
pub(crate) struct HandlerError {
    pub error: String,
    pub message: String,
}
pub use health::AppState;
pub use health::health_check;
pub use metrics::{get_metric_timeseries, get_metrics_catalog, get_metrics_overview};
pub use request::query_request_logs;
pub use request_analytics::{aggregate_request_logs, get_upstream_billing_summary};
pub use time::parse_time_string;
