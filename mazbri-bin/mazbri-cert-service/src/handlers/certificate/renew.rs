use super::common::{CertificateState, extract_client_ip, update_certificate_inventory_metrics};
use super::issue::lookup_preset_dns_names;
use crate::db::CertificateResponse;
use crate::error::CertError;
use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use mazbri_tls::CertMetricsRecorder;
use serde::Deserialize;
use std::net::SocketAddr;
use uuid::Uuid;
#[derive(Debug, Deserialize)]
pub struct RenewCertificateRequest {
    #[serde(default = "default_validity_days")]
    pub validity_days: i64,
}
fn default_validity_days() -> i64 {
    365
}
pub async fn renew_certificate(
    State(state): State<CertificateState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(request): Json<RenewCertificateRequest>,
) -> Response {
    let metrics = CertMetricsRecorder::new("renew");
    let _timer = metrics.start_timer();
    match renew_certificate_inner(
        state.clone(),
        headers,
        Some(addr),
        id,
        request.validity_days,
    )
    .await
    {
        Ok(response) => {
            metrics.record_renewal_success();
            update_certificate_inventory_metrics(&state.repo).await;
            response
        }
        Err(e) => {
            metrics.record_renewal_failed();
            e.into_response()
        }
    }
}
async fn renew_certificate_inner(
    state: CertificateState,
    headers: HeaderMap,
    socket_addr: Option<SocketAddr>,
    id: Uuid,
    validity_days: i64,
) -> Result<Response, CertError> {
    let existing = state.repo.get_by_id(id).await?;
    if existing.revoked {
        return Err(CertError::AlreadyRevoked(id.to_string()));
    }
    let dns_names = if existing.dns_names.is_empty() {
        lookup_preset_dns_names(&state, &existing.subject).await?
    } else {
        existing.dns_names.clone()
    };
    let ca_manager = state.base_authorization.require_issuance().await?;
    let (cert_pem, key_encrypted) = ca_manager.issue_certificate(&existing.subject, &dns_names)?;
    let base_time = if existing.expires_at > Utc::now() {
        existing.expires_at
    } else {
        Utc::now()
    };
    let expires_at = base_time + chrono::Duration::days(validity_days);
    let actor_ip =
        extract_client_ip(&headers, socket_addr).and_then(|ip| ip.parse::<std::net::IpAddr>().ok());
    let record = state
        .repo
        .update_certificate(id, &cert_pem, &key_encrypted, expires_at, &dns_names)
        .await?;
    let key_pem = ca_manager.decrypt_key(&record.key_pem)?;
    let _ = state
        .repo
        .create_audit_log(
            "renew",
            actor_ip,
            Some(&record.subject),
            Some(record.id),
            "success",
            None,
        )
        .await;
    tracing::info!(
        subject = % record.subject, cert_id = % record.id, dns_names = ? dns_names, ip =
        ? actor_ip, "证书已续期"
    );
    Ok((
        StatusCode::OK,
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
