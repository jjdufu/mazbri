mod audit_analytics;
mod audit_query;
mod audit_types;
mod health;
mod metrics;
mod request;
mod request_analytics;
#[derive(Debug, serde::Serialize)]
pub(crate) struct HandlerError {
    pub error: String,
    pub message: String,
}
pub use audit_analytics::get_audit_analytics;
pub use audit_query::query_audit_logs;
pub use audit_types::{
    AuditAnalyticsRequest, AuditAnalyticsResponse, AuditAnalyticsTimelineItem, AuditLogItem,
    AuditQueryRequest, AuditQueryResponse, parse_time_string,
};
pub use health::AppState;
pub use health::health_check;
pub use metrics::{get_metric_timeseries, get_metrics_catalog, get_metrics_overview};
pub use request::query_request_logs;
pub use request_analytics::{aggregate_request_logs, get_upstream_billing_summary};
