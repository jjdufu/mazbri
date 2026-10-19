use super::common::{CertificateState, extract_client_ip};
use crate::db::{VerifyCertificateRequest, VerifyCertificateResponse};
use crate::error::CertError;
use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
pub async fn verify_certificate(
    State(state): State<CertificateState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<VerifyCertificateRequest>,
) -> Response {
    match verify_certificate_inner(state, headers, Some(addr), request).await {
        Ok(response) => response,
        Err(e) => e.into_response(),
    }
}
async fn verify_certificate_inner(
    state: CertificateState,
    headers: HeaderMap,
    socket_addr: Option<SocketAddr>,
    request: VerifyCertificateRequest,
) -> Result<Response, CertError> {
    let actor_ip =
        extract_client_ip(&headers, socket_addr).and_then(|ip| ip.parse::<std::net::IpAddr>().ok());
    let cert_pem = request.cert_pem.trim();
    if !cert_pem.contains("-----BEGIN CERTIFICATE-----") {
        return Ok(invalid_pem_response());
    }
    let fingerprint = match compute_cert_fingerprint(cert_pem) {
        Ok(fp) => fp,
        Err(_) => return Ok(invalid_pem_response()),
    };
    let cert = state.repo.find_by_fingerprint(&fingerprint).await?;
    let response = match cert {
        Some(c) => build_verification_response(&c),
        None => certificate_not_found_response(),
    };
    log_verification_audit(&state, actor_ip, &response).await;
    Ok((StatusCode::OK, Json(response)).into_response())
}
fn invalid_pem_response() -> Response {
    (
        StatusCode::OK,
        Json(VerifyCertificateResponse {
            valid: false,
            subject: None,
            expires_at: None,
            revoked: None,
            error: Some("无效的 PEM 格式".into()),
            matches: None,
        }),
    )
        .into_response()
}
fn compute_cert_fingerprint(pem: &str) -> Result<Vec<u8>, String> {
    use x509_parser::pem::parse_x509_pem;
    let (_, pem_obj) = parse_x509_pem(pem.as_bytes()).map_err(|e| format!("invalid PEM: {}", e))?;
    Ok(Sha256::digest(&pem_obj.contents).to_vec())
}
fn build_verification_response(cert: &crate::db::CertificateSummary) -> VerifyCertificateResponse {
    let now = Utc::now();
    let is_expired = cert.expires_at < now;
    let is_revoked = cert.revoked;
    if is_expired {
        expired_certificate_response(cert, is_revoked)
    } else if is_revoked {
        revoked_certificate_response(cert)
    } else {
        valid_certificate_response(cert)
    }
}
fn expired_certificate_response(
    cert: &crate::db::CertificateSummary,
    is_revoked: bool,
) -> VerifyCertificateResponse {
    VerifyCertificateResponse {
        valid: false,
        subject: Some(cert.subject.clone()),
        expires_at: Some(cert.expires_at),
        revoked: Some(is_revoked),
        error: Some("证书已过期".into()),
        matches: None,
    }
}
fn revoked_certificate_response(cert: &crate::db::CertificateSummary) -> VerifyCertificateResponse {
    VerifyCertificateResponse {
        valid: false,
        subject: Some(cert.subject.clone()),
        expires_at: Some(cert.expires_at),
        revoked: Some(true),
        error: Some("证书已被作废".into()),
        matches: None,
    }
}
fn valid_certificate_response(cert: &crate::db::CertificateSummary) -> VerifyCertificateResponse {
    VerifyCertificateResponse {
        valid: true,
        subject: Some(cert.subject.clone()),
        expires_at: Some(cert.expires_at),
        revoked: Some(false),
        error: None,
        matches: None,
    }
}
fn certificate_not_found_response() -> VerifyCertificateResponse {
    VerifyCertificateResponse {
        valid: false,
        subject: None,
        expires_at: None,
        revoked: None,
        error: Some("证书记录未找到".into()),
        matches: None,
    }
}
async fn log_verification_audit(
    state: &CertificateState,
    actor_ip: Option<std::net::IpAddr>,
    response: &VerifyCertificateResponse,
) {
    let _ = state
        .repo
        .create_audit_log(
            "verify",
            actor_ip,
            response.subject.as_deref(),
            None,
            if response.valid { "success" } else { "failure" },
            response.error.as_deref(),
        )
        .await;
}
