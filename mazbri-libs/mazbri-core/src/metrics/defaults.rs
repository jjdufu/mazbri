use super::{MetricsBuilder, register_collector_once};
use crate::circuit_breaker::{
    CIRCUIT_BREAKER_FAILURES, CIRCUIT_BREAKER_STATE, CIRCUIT_BREAKER_SUCCESS_RATE,
};
use prometheus::{CounterVec, HistogramVec, IntGauge, Registry};
use std::sync::LazyLock;
type IntGaugeAtomic = IntGauge;
pub static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::new);
pub static HTTP_REQUESTS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "http_requests_total",
        "Total number of HTTP requests",
        &["method", "path", "status"],
    )
});
pub static HTTP_REQUEST_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "http_request_duration_seconds",
        "HTTP request duration in seconds",
        vec![
            0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ],
        &["method", "path"],
    )
});
pub static ACTIVE_CONNECTIONS: LazyLock<IntGaugeAtomic> = LazyLock::new(|| {
    MetricsBuilder::new().gauge("active_connections", "Number of active connections")
});
pub static DB_QUERY_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "db_query_total",
        "Total number of database queries",
        &["operation", "table"],
    )
});
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(HTTP_REQUESTS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(HTTP_REQUEST_DURATION_SECONDS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(ACTIVE_CONNECTIONS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(DB_QUERY_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CIRCUIT_BREAKER_STATE.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CIRCUIT_BREAKER_FAILURES.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CIRCUIT_BREAKER_SUCCESS_RATE.clone()))?;
    Ok(())
}
