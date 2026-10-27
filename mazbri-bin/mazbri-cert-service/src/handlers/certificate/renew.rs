use super::common::{CertificateState, extract_client_ip, update_certificate_inventory_metrics};
use crate::db::CertificateResponse;
use crate::error::CertError;
use crate::token::scope::{ScopeContext, enforce_owner};
use axum::{
    Json,
    body::Bytes,
    extract::{ConnectInfo, Extension, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use mazbri_tls::CertMetricsRecorder;
use serde::Deserialize;
use std::net::SocketAddr;
use uuid::Uuid;
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
    Extension(scope): Extension<ScopeContext>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    body: Bytes,
) -> Response {
    let metrics = CertMetricsRecorder::new("renew");
    let _timer = metrics.start_timer();
    let validity_days = match parse_renew_body(&headers, &body) {
        Ok(days) => days,
        Err(response) => return *response,
    };
    match renew_certificate_inner(state.clone(), headers, Some(addr), scope, id, validity_days)
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
fn parse_renew_body(headers: &HeaderMap, body: &[u8]) -> Result<i64, Box<Response>> {
    let no_content_type = !headers.contains_key(header::CONTENT_TYPE);
    let empty = body.iter().all(u8::is_ascii_whitespace);
    if no_content_type || empty {
        return Ok(default_validity_days());
    }
    match serde_json::from_slice::<RenewCertificateRequest>(body) {
        Ok(request) => Ok(request.validity_days),
        Err(error) => Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_body",
                    "message": error.to_string()
                })),
            )
                .into_response(),
        )),
    }
}

async fn renew_certificate_inner(
    state: CertificateState,
    headers: HeaderMap,
    socket_addr: Option<SocketAddr>,
    scope: ScopeContext,
    id: Uuid,
    validity_days: i64,
) -> Result<Response, CertError> {
    let existing = state.repo.get_by_id(id).await?;
    if let Err(response) = enforce_owner(scope.mode, scope.caller, &existing.subject, "/renew/{id}")
    {
        return Ok(*response);
    }
    if existing.revoked {
        return Err(CertError::AlreadyRevoked(id.to_string()));
    }
    let dns_names = state.san_policy.sans_for_subject(&existing.subject)?;
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
    crate::audit::note_detail("subject", record.subject.clone());
    crate::audit::note_detail("expires_at", record.expires_at.to_rfc3339());
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
            ca_cert_pem: ca_manager.get_ca_pem().clone(),
            dns_names,
        }),
    )
        .into_response())
}
