use mazbri_core::metrics::{MetricsBuilder, REGISTRY, register_collector_once};
use prometheus::Error as PrometheusError;
use prometheus::{CounterVec, HistogramVec, IntGauge};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicI64, Ordering};
pub static CACHE_OPERATIONS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "cache_operations_total",
        "Total number of cache operations",
        &["operation", "result"],
    )
});
pub static CACHE_OPERATION_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "cache_operation_duration_seconds",
        "Cache operation duration in seconds",
        vec![
            0.0001, 0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0,
        ],
        &["operation"],
    )
});
pub static CACHE_ENTRIES: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge("cache_entries", "Number of entries in the cache")
});
pub static CACHE_MEMORY_BYTES: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge("cache_memory_bytes", "Total memory usage of cache in bytes")
});
pub static CACHE_EVICTIONS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "cache_evictions_total",
        "Total number of cache evictions",
        &["reason"],
    )
});
pub static TENANT_SIZE_ESTIMATES: LazyLock<dashmap::DashMap<String, AtomicI64>> =
    LazyLock::new(dashmap::DashMap::new);
pub fn add_tenant_size_estimate(tenant_id: &str, bytes: i64) {
    if let Some(entry) = TENANT_SIZE_ESTIMATES.get(tenant_id) {
        entry.fetch_add(bytes, Ordering::Relaxed);
    } else {
        TENANT_SIZE_ESTIMATES
            .entry(tenant_id.to_string())
            .or_insert_with(|| AtomicI64::new(0))
            .fetch_add(bytes, Ordering::Relaxed);
    }
}
pub fn reset_tenant_size_estimate(tenant_id: &str) {
    if let Some(entry) = TENANT_SIZE_ESTIMATES.get(tenant_id) {
        entry.store(0, Ordering::Relaxed);
    }
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_cache_metrics() -> Result<(), PrometheusError> {
    register_collector_once(&REGISTRY, Box::new(CACHE_OPERATIONS_TOTAL.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(CACHE_OPERATION_DURATION_SECONDS.clone()),
    )?;
    register_collector_once(&REGISTRY, Box::new(CACHE_ENTRIES.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CACHE_MEMORY_BYTES.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CACHE_EVICTIONS_TOTAL.clone()))?;
    Ok(())
}
pub struct CacheOperationRecorder {
    operation: &'static str,
}
impl CacheOperationRecorder {
    #[must_use]
    pub fn new(operation: &'static str) -> Self {
        Self { operation }
    }
    pub fn record_hit(&self) {
        CACHE_OPERATIONS_TOTAL
            .with_label_values(&[self.operation, "hit"])
            .inc();
    }
    pub fn record_miss(&self) {
        CACHE_OPERATIONS_TOTAL
            .with_label_values(&[self.operation, "miss"])
            .inc();
    }
    pub fn record_error(&self) {
        CACHE_OPERATIONS_TOTAL
            .with_label_values(&[self.operation, "error"])
            .inc();
    }
    pub fn record_success(&self) {
        CACHE_OPERATIONS_TOTAL
            .with_label_values(&[self.operation, "success"])
            .inc();
    }
    pub fn record_not_found(&self) {
        CACHE_OPERATIONS_TOTAL
            .with_label_values(&[self.operation, "not_found"])
            .inc();
    }
    pub fn start_timer(&self) -> prometheus::HistogramTimer {
        CACHE_OPERATION_DURATION_SECONDS
            .with_label_values(&[self.operation])
            .start_timer()
    }
}
pub struct EvictionRecorder {
    reason: &'static str,
}
impl EvictionRecorder {
    #[must_use]
    pub fn new(reason: &'static str) -> Self {
        Self { reason }
    }
    pub fn record(&self) {
        CACHE_EVICTIONS_TOTAL
            .with_label_values(&[self.reason])
            .inc();
    }
}
