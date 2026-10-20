use mazbri_core::metrics::MetricsBuilder;
use mazbri_core::metrics::REGISTRY;
use mazbri_core::metrics::register_collector_once;
use prometheus::{CounterVec, HistogramVec};
use std::sync::LazyLock;

/// `status_class`: 2xx|3xx|4xx|5xx|0
/// result: `ok|timeout|transport|ssrf|auth_token|config|transform`
pub static BYPASS_UPSTREAM_REQUESTS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_upstream_requests_total",
        "Total number of upstream requests executed by the mazbri-bypass",
        &["method", "status_class", "result"],
    )
});

pub static BYPASS_UPSTREAM_ERRORS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_upstream_errors_total",
        "Total number of upstream failures executed by the mazbri-bypass",
        &["method", "kind"],
    )
});

pub static BYPASS_UPSTREAM_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "bypass_upstream_duration_seconds",
        "Duration of mazbri-bypass upstream requests in seconds",
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
        &["method", "result"],
    )
});

pub static BYPASS_SSRF_BLOCK_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_ssrf_block_total",
        "SSRF blocks by reason",
        &["reason"],
    )
});

/// result: `ok|error|timeout|cache_hit|cache_miss`
pub static BYPASS_AUTH_TOKEN_FETCH_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_auth_token_fetch_total",
        "Auth token fetch outcomes",
        &["result"],
    )
});

/// result: hit|miss|invalid
pub static BYPASS_CONFIG_CACHE_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_config_cache_total",
        "Supplier config cache outcomes on bypass",
        &["result"],
    )
});

pub fn register_upstream_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(BYPASS_UPSTREAM_REQUESTS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BYPASS_UPSTREAM_ERRORS_TOTAL.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(BYPASS_UPSTREAM_DURATION_SECONDS.clone()),
    )?;
    register_collector_once(&REGISTRY, Box::new(BYPASS_SSRF_BLOCK_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BYPASS_AUTH_TOKEN_FETCH_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BYPASS_CONFIG_CACHE_TOTAL.clone()))?;
    Ok(())
}

fn status_class(status: u16) -> &'static str {
    match status {
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "0",
    }
}

pub struct UpstreamMetricsRecorder;

impl UpstreamMetricsRecorder {
    pub fn record_response(method: &str, status: reqwest::StatusCode, result: &str) {
        let class = status_class(status.as_u16());
        BYPASS_UPSTREAM_REQUESTS_TOTAL
            .with_label_values(&[method, class, result])
            .inc();
    }

    pub fn record_failure(method: &str, result: &str) {
        BYPASS_UPSTREAM_REQUESTS_TOTAL
            .with_label_values(&[method, "0", result])
            .inc();
        BYPASS_UPSTREAM_ERRORS_TOTAL
            .with_label_values(&[method, result])
            .inc();
    }

    pub fn record_timeout(method: &str) {
        Self::record_failure(method, "timeout");
    }

    pub fn record_transport_error(method: &str) {
        Self::record_failure(method, "transport");
    }

    pub fn record_ssrf(reason: &str) {
        let r = if reason.is_empty() { "blocked" } else { reason };
        // 低基数 reason：截断/归一化
        let reason_label = if r.contains("private") || r.contains("loopback") || r.contains("127.")
        {
            "private_ip"
        } else if r.contains("dns") || r.contains("resolve") {
            "dns"
        } else if r.contains("scheme") || r.contains("protocol") {
            "scheme"
        } else {
            "other"
        };
        BYPASS_SSRF_BLOCK_TOTAL
            .with_label_values(&[reason_label])
            .inc();
    }

    pub fn record_token_fetch(result: &str) {
        BYPASS_AUTH_TOKEN_FETCH_TOTAL
            .with_label_values(&[result])
            .inc();
    }

    pub fn record_config_cache(result: &str) {
        BYPASS_CONFIG_CACHE_TOTAL.with_label_values(&[result]).inc();
    }

    pub fn observe_duration(method: &str, result: &str, seconds: f64) {
        BYPASS_UPSTREAM_DURATION_SECONDS
            .with_label_values(&[method, result])
            .observe(seconds);
    }
}
