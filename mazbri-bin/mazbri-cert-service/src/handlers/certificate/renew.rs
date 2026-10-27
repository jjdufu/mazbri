use super::common::{CertificateState, extract_client_ip, update_certificate_inventory_metrics};
use crate::db::CertificateResponse;
use crate::error::CertError;
use crate::token::scope::{ScopeContext, owner_visible};
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
/// 解析续期请求体。
///
/// - 空 body（或只有空白）：不论有没有 Content-Type，都用默认有效期。客户端续期时通常不带 body。
/// - 非空 body：Content-Type 必须是 JSON（`application/json` 或 `application/*+json`），
///   否则 415，与改造前 `Json` 提取器的行为一致；JSON 无效则 400。
fn parse_renew_body(headers: &HeaderMap, body: &[u8]) -> Result<i64, Box<Response>> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(default_validity_days());
    }
    if !is_json_content_type(headers) {
        return Err(Box::new(
            (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                Json(serde_json::json!({
                    "error": "unsupported_media_type",
                    "message": "请求体需要 Content-Type: application/json"
                })),
            )
                .into_response(),
        ));
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

fn is_json_content_type(headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let essence = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    essence == "application/json"
        || (essence.starts_with("application/") && essence.ends_with("+json"))
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
    if !owner_visible(
        scope.owner_mode,
        scope.caller,
        &existing.subject,
        "/renew/{id}",
    ) {
        // 与真正不存在时的响应完全相同；真实原因只进日志、指标和审计。
        crate::audit::note_failure("not_owner");
        return Err(CertError::NotFound(id.to_string()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(content_type: Option<&'static str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(value) = content_type {
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(value));
        }
        headers
    }

    fn status(result: Result<i64, Box<Response>>) -> Result<i64, StatusCode> {
        result.map_err(|response| response.status())
    }

    #[test]
    fn empty_body_uses_default_with_or_without_content_type() {
        let default = default_validity_days();
        assert_eq!(status(parse_renew_body(&headers(None), b"")), Ok(default));
        assert_eq!(
            status(parse_renew_body(&headers(None), b" \n")),
            Ok(default)
        );
        assert_eq!(
            status(parse_renew_body(&headers(Some("application/json")), b"")),
            Ok(default)
        );
        assert_eq!(
            status(parse_renew_body(&headers(Some("text/plain")), b"")),
            Ok(default)
        );
    }

    #[test]
    fn non_empty_body_requires_json_content_type() {
        let body = br#"{"validity_days":30}"#;
        assert_eq!(
            status(parse_renew_body(&headers(None), body)),
            Err(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        );
        assert_eq!(
            status(parse_renew_body(&headers(Some("text/plain")), body)),
            Err(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        );
        assert_eq!(
            status(parse_renew_body(&headers(Some("application/json")), body)),
            Ok(30)
        );
        assert_eq!(
            status(parse_renew_body(
                &headers(Some("Application/JSON; charset=utf-8")),
                body
            )),
            Ok(30)
        );
        assert_eq!(
            status(parse_renew_body(
                &headers(Some("application/merge-patch+json")),
                body
            )),
            Ok(30)
        );
        assert_eq!(
            status(parse_renew_body(
                &headers(Some("application/json")),
                b"{nope"
            )),
            Err(StatusCode::BAD_REQUEST)
        );
        assert_eq!(
            status(parse_renew_body(
                &headers(Some("application/json")),
                br#"{"unknown":1}"#
            )),
            Err(StatusCode::BAD_REQUEST)
        );
    }
}
