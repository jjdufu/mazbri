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
/// 鉴权失败（API Key 校验在 auth-service 记，JWT / 请求签名在网关记，同一次失败只记一处）。
pub static AUTH_FAILURES_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "auth_failures_total",
        "Authentication failures by bounded reason",
        &["reason"],
    )
});

/// `auth_failures_total` 的 reason（有限枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFailureReason {
    Missing,
    Invalid,
    Expired,
    Revoked,
    Signature,
    Replay,
    IpDenied,
}

impl AuthFailureReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Invalid => "invalid",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
            Self::Signature => "signature",
            Self::Replay => "replay",
            Self::IpDenied => "ip_denied",
        }
    }

    /// 凭证类错误映射到 reason；基础设施错误（数据库等）不是鉴权失败，返回 `None`。
    #[must_use]
    pub const fn from_auth_error(error: &crate::AuthError) -> Option<Self> {
        use crate::AuthError as E;
        Some(match error {
            E::Core(_) => return None,
            E::InvalidApiKeyFormat(_)
            | E::ApiKeyNotFound(_)
            | E::InvalidJwtToken(_)
            | E::TenantNotFound(_) => Self::Invalid,
            E::ApiKeyExpired(_) | E::JwtTokenExpired(_) => Self::Expired,
            E::ApiKeyDisabled(_) | E::TenantSuspended(_) => Self::Revoked,
            E::IpNotWhitelisted(_) => Self::IpDenied,
        })
    }
}

pub fn record_auth_failure(reason: AuthFailureReason) {
    AUTH_FAILURES_TOTAL
        .with_label_values(&[reason.as_str()])
        .inc();
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_auth_metrics() -> Result<(), PrometheusError> {
    register_collector_once(&REGISTRY, Box::new(AUTH_KEY_VALIDATION_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(AUTH_FAILURES_TOTAL.clone()))?;
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
