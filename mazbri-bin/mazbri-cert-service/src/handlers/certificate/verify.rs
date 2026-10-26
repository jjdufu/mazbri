use super::common::CertificateState;
use crate::db::{VerifyCertificateRequest, VerifyCertificateResponse};
use crate::error::CertError;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use sha2::{Digest, Sha256};
pub async fn verify_certificate(
    State(state): State<CertificateState>,
    Json(request): Json<VerifyCertificateRequest>,
) -> Response {
    match verify_certificate_inner(state, request).await {
        Ok(response) => response,
        Err(e) => e.into_response(),
    }
}
async fn verify_certificate_inner(
    state: CertificateState,
    request: VerifyCertificateRequest,
) -> Result<Response, CertError> {
    let cert_pem = request.cert_pem.trim();
    if !cert_pem.contains("-----BEGIN CERTIFICATE-----") {
        crate::audit::note_failure("invalid_pem");
        return Ok(invalid_pem_response());
    }
    let Ok(fingerprint) = compute_cert_fingerprint(cert_pem) else {
        crate::audit::note_failure("invalid_pem");
        return Ok(invalid_pem_response());
    };
    let cert = state.repo.find_by_fingerprint(&fingerprint).await?;
    let response = match cert {
        Some(c) => build_verification_response(&c),
        None => certificate_not_found_response(),
    };
    note_verification_outcome(&response);
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
    let (_, pem_obj) = parse_x509_pem(pem.as_bytes()).map_err(|e| format!("invalid PEM: {e}"))?;
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
/// 只有失败才会被审计层落库（`/verify` 为 failures-only 接口）。
fn note_verification_outcome(response: &VerifyCertificateResponse) {
    if response.valid {
        return;
    }
    let reason = if response.subject.is_none() {
        "not_found"
    } else if response.expires_at.is_some_and(|at| at < Utc::now()) {
        "expired"
    } else {
        "revoked"
    };
    crate::audit::note_failure(reason);
    if let Some(subject) = &response.subject {
        crate::audit::note_detail("subject", subject.clone());
    }
}
