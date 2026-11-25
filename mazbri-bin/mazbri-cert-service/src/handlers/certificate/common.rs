use crate::base_authorization::BaseAuthorizationState;
use crate::db::CertificateRepository;
use crate::error::CertError;
use axum::http::HeaderMap;
use chrono::TimeDelta;
use mazbri_core::resolve_client_ip;
use mazbri_tls::CertMetricsRecorder;
use std::net::SocketAddr;

pub const MIN_CERT_VALIDITY_DAYS: i64 = 1;
pub const MAX_CERT_VALIDITY_DAYS: i64 = 3650;

#[derive(Clone)]
pub struct CertificateState {
    pub repo: std::sync::Arc<CertificateRepository>,
    pub base_authorization: std::sync::Arc<BaseAuthorizationState>,
    pub san_policy: std::sync::Arc<crate::san::SanPolicy>,
}

/// 校验签发与续期的 `validity_days` 边界（`1..=3650` 且不超过 CA 剩余有效天数），
/// 并通过 `TimeDelta::try_days` 安全构造时间跨度，彻底消除算术溢出 Panic。
///
/// # Errors
///
/// 当 `validity_days` 越界或超过 CA 剩余有效天数时返回 `CertError::InvalidRequest`（映射为 `400 Bad Request`）。
pub fn validate_validity_days(
    validity_days: i64,
    ca_remaining_days: Option<i64>,
) -> Result<TimeDelta, CertError> {
    if !(MIN_CERT_VALIDITY_DAYS..=MAX_CERT_VALIDITY_DAYS).contains(&validity_days) {
        return Err(CertError::InvalidRequest(format!(
            "validity_days 必须在 {MIN_CERT_VALIDITY_DAYS} 到 {MAX_CERT_VALIDITY_DAYS} 之间"
        )));
    }
    if let Some(max_ca_days) = ca_remaining_days
        && validity_days > max_ca_days
    {
        return Err(CertError::InvalidRequest(format!(
            "validity_days ({validity_days}) 不能超过 CA 剩余有效天数 ({max_ca_days})"
        )));
    }
    TimeDelta::try_days(validity_days)
        .ok_or_else(|| CertError::InvalidRequest("validity_days 超出时间跨度上限".into()))
}

#[must_use]
pub fn extract_client_ip(headers: &HeaderMap, socket_addr: Option<SocketAddr>) -> Option<String> {
    let resolved = resolve_client_ip(headers, socket_addr);
    (resolved != "unknown").then_some(resolved)
}
pub async fn update_certificate_inventory_metrics(repo: &CertificateRepository) {
    let recorder = CertMetricsRecorder::new("inventory");
    if let Ok(active_count) = repo.count_active_certificates().await {
        recorder.set_active_count(active_count);
    }
    if let Ok(Some(min_days)) = repo.min_active_expiry_days().await {
        recorder.set_min_expiry_days(min_days);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ca::CaManager;
    use crate::config::CertConfig;
    use chrono::Utc;
    use std::sync::Arc;

    #[test]
    fn rejects_non_positive_or_excessive_validity_days_without_panic() {
        for bad in [i64::MIN, -5, 0, 3651, 100_000, i64::MAX] {
            let err = validate_validity_days(bad, None).expect_err("must reject invalid days");
            assert!(matches!(err, CertError::InvalidRequest(_)), "{bad}: {err}");
        }
        assert_eq!(validate_validity_days(1, None).expect("1 day").num_days(), 1);
        assert_eq!(
            validate_validity_days(3650, None)
                .expect("3650 days")
                .num_days(),
            3650
        );
    }

    #[test]
    fn enforces_ca_remaining_validity_cap() {
        let ca = CaManager::new(Arc::new(CertConfig {
            ca_validity_days: 30,
            cert_validity_days: 10,
            key_password: Some("test-ca-password".to_string()),
        }))
        .expect("ca");
        let remaining = ca.ca_remaining_days(Utc::now());
        assert!(remaining.is_some_and(|days| (29..=30).contains(&days)));
        assert!(validate_validity_days(29, remaining).is_ok());
        assert!(matches!(
            validate_validity_days(31, remaining),
            Err(CertError::InvalidRequest(_))
        ));
        assert!(matches!(
            validate_validity_days(1, Some(0)),
            Err(CertError::InvalidRequest(_))
        ));
    }
}
