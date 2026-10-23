use super::{MetricsBuilder, REGISTRY, register_collector_once};
use prometheus::Error as PrometheusError;
use prometheus::{CounterVec, IntGauge};
use std::sync::LazyLock;
pub static CBPRO_STATE_TRANSITIONS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "cbpro_state_transitions_total",
        "Total number of circuit breaker state transitions",
        &["from_state", "to_state"],
    )
});
pub static CBPRO_PROTECTION_TRIGGERS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "cbpro_protection_triggers_total",
        "Total number of protection triggers",
        &["trigger_type"],
    )
});
pub static CBPRO_CURRENT_STATE: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge(
        "cbpro_current_state",
        "Current circuit breaker state (0=closed, 1=half_open, 2=open)",
    )
});
pub static CBPRO_ERROR_RATE_PERCENTAGE: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge(
        "cbpro_error_rate_percentage",
        "Current error rate as percentage (0-100)",
    )
});
pub static CBPRO_CONFIG_CHANGES_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "cbpro_config_changes_total",
        "Total number of circuit breaker configuration changes",
        &["action"],
    )
});
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_cbpro_metrics() -> Result<(), PrometheusError> {
    register_collector_once(&REGISTRY, Box::new(CBPRO_STATE_TRANSITIONS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CBPRO_PROTECTION_TRIGGERS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CBPRO_CURRENT_STATE.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CBPRO_ERROR_RATE_PERCENTAGE.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CBPRO_CONFIG_CHANGES_TOTAL.clone()))?;
    Ok(())
}
pub struct CbProMetricsRecorder;
impl CbProMetricsRecorder {
    pub fn record_state_transition(from: &str, to: &str) {
        CBPRO_STATE_TRANSITIONS_TOTAL
            .with_label_values(&[from, to])
            .inc();
    }
    pub fn record_protection_trigger(trigger_type: &str) {
        CBPRO_PROTECTION_TRIGGERS_TOTAL
            .with_label_values(&[trigger_type])
            .inc();
    }
    pub fn set_current_state(state: i64) {
        CBPRO_CURRENT_STATE.set(state);
    }
    pub fn set_error_rate(rate: i64) {
        CBPRO_ERROR_RATE_PERCENTAGE.set(rate.clamp(0, 100));
    }
    pub fn record_config_change(action: &str) {
        CBPRO_CONFIG_CHANGES_TOTAL
            .with_label_values(&[action])
            .inc();
    }
}
