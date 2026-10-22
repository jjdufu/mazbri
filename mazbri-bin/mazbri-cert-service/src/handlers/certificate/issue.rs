use super::common::{CertificateState, extract_client_ip, update_certificate_inventory_metrics};
use crate::db::{CertificateResponse, IssueCertificateRequest};
use crate::error::CertError;
use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use mazbri_tls::CertMetricsRecorder;
use std::net::SocketAddr;
use tracing::info;
pub async fn issue_certificate(
    State(state): State<CertificateState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<IssueCertificateRequest>,
) -> Response {
    let metrics = CertMetricsRecorder::new("issue");
    let _timer = metrics.start_timer();
    match issue_certificate_inner(state.clone(), headers, Some(addr), request).await {
        Ok(response) => {
            metrics.record_issuance_success("service");
            update_certificate_inventory_metrics(&state.repo).await;
            response
        }
        Err(e) => {
            metrics.record_issuance_failed("service");
            e.into_response()
        }
    }
}
async fn issue_certificate_inner(
    state: CertificateState,
    headers: HeaderMap,
    socket_addr: Option<SocketAddr>,
    request: IssueCertificateRequest,
) -> Result<Response, CertError> {
    let actor_ip =
        extract_client_ip(&headers, socket_addr).and_then(|ip| ip.parse::<std::net::IpAddr>().ok());
    let subject = request.subject.trim();
    // 非规范名 → CertError::SubjectNotCanonical
    let dns_names = state.san_policy.sans_for_subject(subject)?;
    if state.repo.subject_exists(subject).await? {
        return Err(CertError::InvalidSubject(format!(
            "'{subject}' 的证书已存在"
        )));
    }
    let ca_manager = state.base_authorization.require_issuance().await?;
    let (cert_pem, key_encrypted) = ca_manager.issue_certificate(subject, &dns_names)?;
    let expires_at = Utc::now() + chrono::Duration::days(request.validity_days);
    let record = state
        .repo
        .create_certificate(
            subject,
            &cert_pem,
            &key_encrypted,
            expires_at,
            &dns_names,
            actor_ip,
        )
        .await?;
    let key_pem = ca_manager.decrypt_key(&record.key_pem)?;
    let _ = state
        .repo
        .create_audit_log(
            "issue",
            actor_ip,
            Some(subject),
            Some(record.id),
            "success",
            None,
        )
        .await;
    info!(
        subject = % subject, cert_id = % record.id, dns_names = ? dns_names, ip = ?
        actor_ip, "证书已发放"
    );
    Ok((
        StatusCode::CREATED,
        Json(CertificateResponse {
            id: record.id,
            subject: record.subject,
            issued_at: record.issued_at,
            expires_at: record.expires_at,
            cert_pem,
            key_pem,
            ca_cert_pem: ca_manager.get_ca_pem().clone(),
            dns_names,
        }),
    )
        .into_response())
}
