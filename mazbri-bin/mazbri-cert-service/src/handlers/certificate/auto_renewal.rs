use super::common::{CertificateState, extract_client_ip};
use crate::db::{AutoRenewalConfigResponse, ConfigureAutoRenewalRequest};
use crate::error::CertError;
use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;
use uuid::Uuid;
pub async fn get_auto_renewal_config(
    State(state): State<CertificateState>,
    Path(id): Path<Uuid>,
) -> Response {
    match get_auto_renewal_config_inner(state, id).await {
        Ok(response) => response,
        Err(e) => e.into_response(),
    }
}
async fn get_auto_renewal_config_inner(
    state: CertificateState,
    id: Uuid,
) -> Result<Response, CertError> {
    let cert = state.repo.get_by_id(id).await?;
    let config = state.repo.get_auto_renewal_config(id).await?;
    let response = if let Some(cfg) = config {
        AutoRenewalConfigResponse {
            certificate_id: id,
            enabled: cfg.enabled,
            threshold_days: cfg.threshold_days,
            validity_days: cfg.validity_days,
            last_renewal_at: cfg.last_renewal_at,
            next_renewal_at: cfg.next_renewal_at,
            certificate_expires_at: cert.expires_at,
        }
    } else {
        AutoRenewalConfigResponse {
            certificate_id: id,
            enabled: false,
            threshold_days: 30,
            validity_days: 365,
            last_renewal_at: None,
            next_renewal_at: None,
            certificate_expires_at: cert.expires_at,
        }
    };
    Ok((StatusCode::OK, Json(response)).into_response())
}
pub async fn configure_auto_renewal(
    State(state): State<CertificateState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(request): Json<ConfigureAutoRenewalRequest>,
) -> Response {
    match configure_auto_renewal_inner(state, headers, Some(addr), id, request).await {
        Ok(response) => response,
        Err(e) => e.into_response(),
    }
}
async fn configure_auto_renewal_inner(
    state: CertificateState,
    headers: HeaderMap,
    socket_addr: Option<SocketAddr>,
    id: Uuid,
    request: ConfigureAutoRenewalRequest,
) -> Result<Response, CertError> {
    if request.threshold_days < 1 || request.threshold_days > 365 {
        return Err(CertError::InvalidRequest(
            "阈值天数必须在 1 到 365 之间".into(),
        ));
    }
    if request.validity_days < 1 || request.validity_days > 36135 {
        return Err(CertError::InvalidRequest(
            "有效期天数必须在 1 到 36135（99 年）之间".into(),
        ));
    }
    let actor_ip =
        extract_client_ip(&headers, socket_addr).and_then(|ip| ip.parse::<std::net::IpAddr>().ok());
    let cert = state.repo.get_by_id(id).await?;
    if cert.revoked {
        return Err(CertError::AlreadyRevoked(id.to_string()));
    }
    let config = state
        .repo
        .upsert_auto_renewal_config(
            id,
            request.enabled,
            request.threshold_days,
            request.validity_days,
        )
        .await?;
    let _ = state
        .repo
        .create_audit_log(
            "configure_auto_renewal",
            actor_ip,
            Some(&cert.subject),
            Some(id),
            "success",
            None,
        )
        .await;
    tracing::info!(
        cert_id = % id, subject = % cert.subject, enabled = request.enabled,
        threshold_days = request.threshold_days, validity_days = request.validity_days,
        ip = ? actor_ip, "自动续期已配置"
    );
    let response = AutoRenewalConfigResponse {
        certificate_id: id,
        enabled: config.enabled,
        threshold_days: config.threshold_days,
        validity_days: config.validity_days,
        last_renewal_at: config.last_renewal_at,
        next_renewal_at: config.next_renewal_at,
        certificate_expires_at: cert.expires_at,
    };
    Ok((StatusCode::OK, Json(response)).into_response())
}
