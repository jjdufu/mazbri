use mazbri_core::metrics::{MetricsBuilder, REGISTRY, register_collector_once};
use prometheus::{CounterVec, Error as PrometheusError};
use std::sync::LazyLock;
pub static ANALYTICS_LOG_INGESTION_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "analytics_log_ingestion_total",
        "Total number of logs ingested by source",
        &["source"],
    )
});
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_analytics_metrics() -> Result<(), PrometheusError> {
    register_collector_once(&REGISTRY, Box::new(ANALYTICS_LOG_INGESTION_TOTAL.clone()))?;
    Ok(())
}
