use mazbri_core::metrics::{REGISTRY, register_collector_once};
use prometheus::Error as PrometheusError;
use prometheus::{CounterVec, Opts};
use std::sync::LazyLock;
pub static LOGS_DROPPED_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    CounterVec::new(
        Opts::new(
            "mazbri_logs_dropped_total",
            "Total number of logs dropped due to channel backpressure",
        ),
        &["log_type", "reason"],
    )
    .expect("metric creation should not fail")
});
pub fn inc_logs_dropped(log_type: &str, reason: &str) {
    LOGS_DROPPED_TOTAL
        .with_label_values(&[log_type, reason])
        .inc();
}
pub fn register_log_writer_metrics() -> Result<(), PrometheusError> {
    register_collector_once(&REGISTRY, Box::new(LOGS_DROPPED_TOTAL.clone()))
}
