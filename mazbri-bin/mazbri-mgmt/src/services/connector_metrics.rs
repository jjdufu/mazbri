//! Prometheus metrics for Upstream Connector Engine (FR-W12).

use mazbri_core::metrics::{MetricsBuilder, REGISTRY, register_collector_once};
use prometheus::CounterVec;
use std::sync::LazyLock;

/// result: `ok|fail|rate_limited`
pub static CONNECTOR_RUNS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "connector_runs_total",
        "Connector probe / live runs",
        &["mode", "result"],
    )
});

/// result: ok|fail|force
pub static CONNECTOR_PUBLISH_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "connector_publish_total",
        "Connector publish outcomes",
        &["mode", "result"],
    )
});

pub static CONNECTOR_AUTH_FAIL_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "connector_auth_fail_total",
        "Connector auth stage failures",
        &["mode"],
    )
});

pub static CONNECTOR_SSRF_BLOCK_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "connector_ssrf_block_total",
        "Connector SSRF blocks",
        &["mode"],
    )
});

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_connector_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(CONNECTOR_RUNS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CONNECTOR_PUBLISH_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CONNECTOR_AUTH_FAIL_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CONNECTOR_SSRF_BLOCK_TOTAL.clone()))?;
    Ok(())
}

pub fn record_run(mode: &str, ok: bool, error_code: Option<&str>) {
    let result = if ok {
        "ok"
    } else if error_code == Some("RATE_LIMITED") {
        "rate_limited"
    } else {
        "fail"
    };
    CONNECTOR_RUNS_TOTAL
        .with_label_values(&[mode, result])
        .inc();
    if !ok {
        if error_code == Some("AUTH_TOKEN_FAILED") {
            CONNECTOR_AUTH_FAIL_TOTAL.with_label_values(&[mode]).inc();
        }
        if error_code == Some("SSRF_BLOCKED") {
            CONNECTOR_SSRF_BLOCK_TOTAL.with_label_values(&[mode]).inc();
        }
    }
}

pub fn record_publish(mode: &str, force: bool, ok: bool) {
    let result = if !ok {
        "fail"
    } else if force {
        "force"
    } else {
        "ok"
    };
    CONNECTOR_PUBLISH_TOTAL
        .with_label_values(&[mode, result])
        .inc();
}
