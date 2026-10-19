use mazbri_core::metrics::{MetricsBuilder, REGISTRY, register_collector_once};
use prometheus::Error as PrometheusError;
use prometheus::{CounterVec, HistogramVec, IntCounter, IntGauge};
use std::sync::LazyLock;
pub static AUTH_KEY_VALIDATION_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "auth_key_validation_total",
        "Total API key validations",
        &["result", "credential_type"],
    )
});
pub static AUTH_TOKEN_ISSUANCE_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "auth_token_issuance_total",
        "Total tokens issued",
        &["token_type", "operation"],
    )
});
pub static AUTH_VALIDATION_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "auth_validation_duration_seconds",
        "Validation operation duration in seconds",
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0],
        &["validation_type"],
    )
});
pub static AUTH_ACTIVE_SESSIONS: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge("auth_active_sessions", "Number of active sessions")
});
pub static AUTH_LOGIN_ATTEMPTS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "auth_login_attempts_total",
        "Total login attempts",
        &["result", "method"],
    )
});
pub static API_KEY_LAST_USED_DROPPED_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::new(
        "api_key_last_used_dropped_total",
        "Total number of last_used records dropped due to buffer capacity",
    )
    .expect("api_key_last_used_dropped_total metric definition is valid")
});
pub fn register_auth_metrics() -> Result<(), PrometheusError> {
    register_collector_once(&REGISTRY, Box::new(AUTH_KEY_VALIDATION_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(AUTH_TOKEN_ISSUANCE_TOTAL.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(AUTH_VALIDATION_DURATION_SECONDS.clone()),
    )?;
    register_collector_once(&REGISTRY, Box::new(AUTH_ACTIVE_SESSIONS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(AUTH_LOGIN_ATTEMPTS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(API_KEY_LAST_USED_DROPPED_TOTAL.clone()))?;
    Ok(())
}
pub struct KeyValidationRecorder {
    credential_type: &'static str,
}
impl KeyValidationRecorder {
    pub fn new(credential_type: &'static str) -> Self {
        Self { credential_type }
    }
    pub fn record_valid(&self) {
        AUTH_KEY_VALIDATION_TOTAL
            .with_label_values(&["valid", self.credential_type])
            .inc();
    }
    pub fn record_invalid_format(&self) {
        AUTH_KEY_VALIDATION_TOTAL
            .with_label_values(&["invalid_format", self.credential_type])
            .inc();
    }
    pub fn record_not_found(&self) {
        AUTH_KEY_VALIDATION_TOTAL
            .with_label_values(&["not_found", self.credential_type])
            .inc();
    }
    pub fn record_expired(&self) {
        AUTH_KEY_VALIDATION_TOTAL
            .with_label_values(&["expired", self.credential_type])
            .inc();
    }
    pub fn record_disabled(&self) {
        AUTH_KEY_VALIDATION_TOTAL
            .with_label_values(&["disabled", self.credential_type])
            .inc();
    }
    pub fn start_timer(&self) -> prometheus::HistogramTimer {
        AUTH_VALIDATION_DURATION_SECONDS
            .with_label_values(&["api_key"])
            .start_timer()
    }
}
pub struct TokenRecorder {
    token_type: &'static str,
}
impl TokenRecorder {
    pub fn new(token_type: &'static str) -> Self {
        Self { token_type }
    }
    pub fn record_issued(&self) {
        AUTH_TOKEN_ISSUANCE_TOTAL
            .with_label_values(&[self.token_type, "issued"])
            .inc();
    }
    pub fn record_refreshed(&self) {
        AUTH_TOKEN_ISSUANCE_TOTAL
            .with_label_values(&[self.token_type, "refreshed"])
            .inc();
    }
    pub fn record_validated(&self) {
        AUTH_TOKEN_ISSUANCE_TOTAL
            .with_label_values(&[self.token_type, "validated"])
            .inc();
    }
}
pub struct LoginAttemptsRecorder {
    method: &'static str,
}
impl LoginAttemptsRecorder {
    pub fn new(method: &'static str) -> Self {
        Self { method }
    }
    pub fn record_success(&self) {
        AUTH_LOGIN_ATTEMPTS_TOTAL
            .with_label_values(&["success", self.method])
            .inc();
    }
    pub fn record_failed(&self) {
        AUTH_LOGIN_ATTEMPTS_TOTAL
            .with_label_values(&["failed", self.method])
            .inc();
    }
}
