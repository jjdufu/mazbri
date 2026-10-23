use super::state::CircuitState;
use prometheus::{IntCounterVec, IntGaugeVec, Opts};
use std::sync::LazyLock;
use tracing::debug;
pub static CIRCUIT_BREAKER_STATE: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    IntGaugeVec::new(
        Opts::new(
            "circuit_breaker_state",
            "断路器状态 (0=闭合, 1=半开, 2=打开)",
        ),
        &["name"],
    )
    .expect("断路器状态指标定义有效")
});
pub static CIRCUIT_BREAKER_FAILURES: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new("circuit_breaker_failures_total", "断路器失败总数"),
        &["name", "error_type"],
    )
    .expect("断路器失败指标定义有效")
});
pub static CIRCUIT_BREAKER_SUCCESS_RATE: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    IntGaugeVec::new(
        Opts::new("circuit_breaker_success_rate", "断路器成功率百分比 (0-100)"),
        &["name"],
    )
    .expect("断路器成功率指标定义有效")
});
pub(crate) fn record_state(name: &str, state: CircuitState) {
    CIRCUIT_BREAKER_STATE
        .with_label_values(&[name])
        .set(state.as_metric_value());
    debug!(
        circuit_breaker = % name, state = % state, metric_value = state
        .as_metric_value(), "Circuit breaker state recorded"
    );
}
pub(crate) fn record_failure(name: &str, error: &str) {
    let error_type = classify_error(error);
    CIRCUIT_BREAKER_FAILURES
        .with_label_values(&[name, error_type])
        .inc();
    debug!(
        circuit_breaker = % name, error_type = % error_type,
        "Circuit breaker failure recorded"
    );
}
pub(crate) fn record_success_rate(name: &str, success_count: u64, total_requests: u64) {
    if total_requests > 0 {
        let rate =
            crate::num::u64_to_f64(success_count) / crate::num::u64_to_f64(total_requests) * 100.0;
        CIRCUIT_BREAKER_SUCCESS_RATE
            .with_label_values(&[name])
            .set(crate::num::f64_to_i64(rate));
        debug!(circuit_breaker = % name, success_rate = % rate, "Success rate recorded");
    }
}
pub(crate) fn classify_error(error: &str) -> &'static str {
    let error_lower = error.to_lowercase();
    if error_lower.contains("timeout")
        || error_lower.contains("timed out")
        || error_lower.contains("deadline")
    {
        "timeout"
    } else if error_lower.contains("connection")
        || error_lower.contains("connect")
        || error_lower.contains("network")
    {
        "connection"
    } else {
        "unknown"
    }
}
