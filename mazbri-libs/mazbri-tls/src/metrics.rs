use mazbri_core::metrics::{REGISTRY, register_collector_once};
use prometheus::{CounterVec, HistogramTimer, HistogramVec, IntGauge};
use prometheus::{Error as PrometheusError, Opts};
use std::sync::LazyLock;
pub static CERT_ISSUANCE_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    CounterVec::new(
        Opts::new(
            "cert_issuance_total",
            "Total number of certificate issuances",
        ),
        &["type", "result"],
    )
    .expect("Failed to create CERT_ISSUANCE_TOTAL metric")
});
pub static CERT_RENEWALS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    CounterVec::new(
        Opts::new(
            "cert_renewals_total",
            "Total number of certificate renewals",
        ),
        &["result"],
    )
    .expect("Failed to create CERT_RENEWALS_TOTAL metric")
});
pub static CERT_REVOCATIONS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    CounterVec::new(
        Opts::new(
            "cert_revocations_total",
            "Total number of certificate revocations",
        ),
        &["reason"],
    )
    .expect("Failed to create CERT_REVOCATIONS_TOTAL metric")
});
pub static CERT_ACTIVE_COUNT: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new("cert_active_count", "Number of active certificates")
        .expect("Failed to create CERT_ACTIVE_COUNT metric")
});
pub static CERT_MIN_EXPIRY_DAYS: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new(
        "cert_min_expiry_days",
        "Minimum days until any certificate expires",
    )
    .expect("Failed to create CERT_MIN_EXPIRY_DAYS metric")
});
pub static CERT_OPERATION_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    HistogramVec::new(
        prometheus::HistogramOpts {
            common_opts: prometheus::Opts::new(
                "cert_operation_duration_seconds",
                "Certificate operation duration in seconds",
            ),
            buckets: vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
            ],
        },
        &["operation"],
    )
    .expect("Failed to create CERT_OPERATION_DURATION_SECONDS metric")
});
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_cert_metrics() -> Result<(), PrometheusError> {
    register_collector_once(&REGISTRY, Box::new(CERT_ISSUANCE_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CERT_RENEWALS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CERT_REVOCATIONS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CERT_ACTIVE_COUNT.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CERT_MIN_EXPIRY_DAYS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CERT_OPERATION_DURATION_SECONDS.clone()))?;
    Ok(())
}
pub struct CertMetricsRecorder {
    operation: &'static str,
}
impl CertMetricsRecorder {
    #[must_use]
    pub fn new(operation: &'static str) -> Self {
        Self { operation }
    }
    pub fn record_issuance_success(&self, cert_type: &str) {
        CERT_ISSUANCE_TOTAL
            .with_label_values(&[cert_type, "success"])
            .inc();
    }
    pub fn record_issuance_failed(&self, cert_type: &str) {
        CERT_ISSUANCE_TOTAL
            .with_label_values(&[cert_type, "failed"])
            .inc();
    }
    pub fn record_renewal_success(&self) {
        CERT_RENEWALS_TOTAL.with_label_values(&["success"]).inc();
    }
    pub fn record_renewal_failed(&self) {
        CERT_RENEWALS_TOTAL.with_label_values(&["failed"]).inc();
    }
    pub fn record_renewal_expired(&self) {
        CERT_RENEWALS_TOTAL.with_label_values(&["expired"]).inc();
    }
    pub fn record_revocation(&self, reason: &str) {
        CERT_REVOCATIONS_TOTAL.with_label_values(&[reason]).inc();
    }
    pub fn set_active_count(&self, count: i64) {
        CERT_ACTIVE_COUNT.set(count);
    }
    pub fn set_min_expiry_days(&self, days: i64) {
        CERT_MIN_EXPIRY_DAYS.set(days);
    }
    pub fn start_timer(&self) -> HistogramTimer {
        CERT_OPERATION_DURATION_SECONDS
            .with_label_values(&[self.operation])
            .start_timer()
    }
}
