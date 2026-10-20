use crate::base_authorization::BaseAuthorizationState;
use crate::db::CertificateRepository;
use axum::http::HeaderMap;
use mazbri_core::resolve_client_ip;
use mazbri_tls::CertMetricsRecorder;
use std::net::SocketAddr;
#[derive(Clone)]
pub struct CertificateState {
    pub repo: std::sync::Arc<CertificateRepository>,
    pub base_authorization: std::sync::Arc<BaseAuthorizationState>,
    pub san_policy: std::sync::Arc<crate::san::SanPolicy>,
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
