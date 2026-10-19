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
use tracing::{info, warn};
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
    if subject.is_empty() || subject.len() > 255 {
        return Err(CertError::InvalidSubject("主体必须是 1-255 个字符".into()));
    }
    if state.repo.subject_exists(subject).await? {
        return Err(CertError::InvalidSubject(format!(
            "'{}' 的证书已存在",
            subject
        )));
    }
    let dns_names = if let Some(ref names) = request.dns_names {
        if !names.is_empty() {
            names.clone()
        } else {
            lookup_preset_dns_names(&state, subject).await?
        }
    } else {
        lookup_preset_dns_names(&state, subject).await?
    };
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
            ca_cert_pem: ca_manager.get_ca_pem().to_string(),
            dns_names,
        }),
    )
        .into_response())
}
pub(super) async fn lookup_preset_dns_names(
    state: &CertificateState,
    subject: &str,
) -> Result<Vec<String>, CertError> {
    match state.repo.get_preset_by_service_name(subject).await? {
        Some(preset) => {
            info!(
                subject = % subject, dns_names = ? preset.dns_names,
                "使用预置 DNS 名称"
            );
            Ok(preset.dns_names)
        }
        None => {
            warn!(subject = % subject, "未找到预置 DNS 名称，使用空 SAN");
            Ok(vec![])
        }
    }
}
