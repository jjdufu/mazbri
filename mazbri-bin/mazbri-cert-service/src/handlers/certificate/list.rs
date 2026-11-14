use super::common::CertificateState;
use crate::db::repository::CertListFilter;
use crate::db::{
    CertificateRecord, CertificateStatus, FullCertificateResponse,
    PublicCertificateDetailResponse,
};
use crate::error::CertError;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

fn parse_cert_filters(raw: Option<&str>) -> Result<Vec<CertListFilter>, String> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(Vec::new());
    };
    serde_json::from_str(raw).map_err(|e| format!("Invalid filters JSON: {e}"))
}
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub pagination: PaginationInfo,
}
#[derive(Debug, Serialize)]
pub struct PaginationInfo {
    pub page: i64,
    pub limit: i64,
    pub total: i64,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListCertificatesQuery {
    #[serde(default)]
    pub include_revoked: bool,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub page: Option<i64>,
    /// Free-text keyword over subject / fingerprint hex / `dns_names`.
    #[serde(default)]
    pub q: Option<String>,
    /// JSON array of List Query filters (subject, fingerprint, `dns_names`, `expires_at`, status).
    #[serde(default)]
    pub filters: Option<String>,
}
fn default_limit() -> i64 {
    100
}
pub async fn list_certificates(
    State(state): State<CertificateState>,
    axum::extract::Query(query): axum::extract::Query<ListCertificatesQuery>,
) -> Response {
    let limit = query.limit;
    let offset = match query.page {
        Some(page) if page > 0 => (page - 1) * limit,
        _ => query.offset,
    };
    let page = if offset > 0 && query.page.is_none() {
        (offset / limit) + 1
    } else {
        query.page.unwrap_or(1)
    };
    let q = query
        .q
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let filters = match parse_cert_filters(query.filters.as_deref()) {
        Ok(f) => f,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_filters",
                    "message": msg,
                })),
            )
                .into_response();
        }
    };
    let (records, total) = tokio::join!(
        state
            .repo
            .list_certificates(query.include_revoked, q.as_deref(), &filters, limit, offset),
        state
            .repo
            .count_certificates(query.include_revoked, q.as_deref(), &filters),
    );
    match (records, total) {
        (Ok(data), Ok(total)) => (
            StatusCode::OK,
            Json(PaginatedResponse {
                data,
                pagination: PaginationInfo { page, limit, total },
            }),
        )
            .into_response(),
        (Err(e), _) | (_, Err(e)) => e.into_response(),
    }
}
#[must_use]
pub fn build_public_certificate_detail(
    record: CertificateRecord,
) -> PublicCertificateDetailResponse {
    let status = calculate_certificate_status(&record.revoked, &record.expires_at);
    let fingerprint = record.fingerprint.as_deref().map(hex::encode);
    PublicCertificateDetailResponse {
        id: record.id,
        subject: record.subject,
        issued_at: record.issued_at,
        expires_at: record.expires_at,
        revoked: record.revoked,
        revoked_at: record.revoked_at,
        renewal_count: record.renewal_count,
        dns_names: record.dns_names,
        fingerprint,
        status: status.as_str().to_string(),
        cert_pem: record.cert_pem,
    }
}

pub async fn get_certificate(
    State(state): State<CertificateState>,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    match state.repo.get_by_id(id).await {
        Ok(record) => (StatusCode::OK, Json(build_public_certificate_detail(record))).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn list_certificate_subjects(State(state): State<CertificateState>) -> Response {
    match state.repo.list_active_subjects().await {
        Ok(subjects) => (StatusCode::OK, Json(subjects)).into_response(),
        Err(e) => e.into_response(),
    }
}
pub async fn get_certificate_by_subject(
    State(state): State<CertificateState>,
    Path(subject): Path<String>,
) -> Response {
    match get_certificate_by_subject_inner(state, subject).await {
        Ok(response) => response,
        Err(e) => e.into_response(),
    }
}
async fn get_certificate_by_subject_inner(
    state: CertificateState,
    subject: String,
) -> Result<Response, CertError> {
    let subject = subject.trim();
    let record = state
        .repo
        .get_by_subject(subject)
        .await?
        .ok_or_else(|| CertError::NotFound(format!("{subject} (no active certificate)")))?;
    let ca_manager = state.base_authorization.require_ca_manager().await?;
    let key_pem = ca_manager.decrypt_key(&record.key_pem)?;
    let status = calculate_certificate_status(&record.revoked, &record.expires_at);
    tracing::info!(
        subject = % subject, cert_id = % record.id, status = status.as_str(),
        "按主体获取证书"
    );
    Ok((
        StatusCode::OK,
        Json(FullCertificateResponse {
            id: record.id,
            subject: record.subject,
            cert_pem: record.cert_pem,
            key_pem,
            ca_cert_pem: ca_manager.get_ca_pem().clone(),
            dns_names: record.dns_names,
            issued_at: record.issued_at,
            expires_at: record.expires_at,
            status: status.as_str().to_string(),
        }),
    )
        .into_response())
}
pub async fn get_certificate_full(
    State(state): State<CertificateState>,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    match get_certificate_full_inner(state, id).await {
        Ok(response) => response,
        Err(e) => e.into_response(),
    }
}
async fn get_certificate_full_inner(
    state: CertificateState,
    id: uuid::Uuid,
) -> Result<Response, CertError> {
    let record = state.repo.get_by_id(id).await?;
    let ca_manager = state.base_authorization.require_ca_manager().await?;
    let key_pem = ca_manager.decrypt_key(&record.key_pem)?;
    let status = calculate_certificate_status(&record.revoked, &record.expires_at);
    tracing::info!(
        cert_id = % id, subject = % record.subject, status = status.as_str(),
        "获取完整证书"
    );
    Ok((
        StatusCode::OK,
        Json(FullCertificateResponse {
            id: record.id,
            subject: record.subject,
            cert_pem: record.cert_pem,
            key_pem,
            ca_cert_pem: ca_manager.get_ca_pem().clone(),
            dns_names: record.dns_names,
            issued_at: record.issued_at,
            expires_at: record.expires_at,
            status: status.as_str().to_string(),
        }),
    )
        .into_response())
}
#[must_use]
pub fn calculate_certificate_status(
    revoked: &bool,
    expires_at: &chrono::DateTime<Utc>,
) -> CertificateStatus {
    if *revoked {
        return CertificateStatus::Revoked;
    }
    let now = Utc::now();
    let threshold = now + Duration::days(30);
    if *expires_at < now {
        CertificateStatus::Expired
    } else if *expires_at < threshold {
        CertificateStatus::ExpiringSoon
    } else {
        CertificateStatus::Valid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{CertificateSummary, CertificateSummaryRow};
    use uuid::Uuid;

    fn sample_record(revoked: bool) -> CertificateRecord {
        let now = Utc::now();
        CertificateRecord {
            id: Uuid::nil(),
            subject: "mazbri-gateway".to_string(),
            issued_at: now - Duration::days(5),
            expires_at: now + Duration::days(360),
            revoked,
            revoked_at: revoked.then_some(now),
            cert_pem: "-----BEGIN CERTIFICATE-----\nMIIB...\n-----END CERTIFICATE-----".to_string(),
            key_pem: "encrypted-private-key-material-must-never-leak".to_string(),
            dns_names: vec!["mazbri-gateway".to_string(), "gateway.internal".to_string()],
            created_by_ip: None,
            renewal_count: 2,
            fingerprint: Some(vec![0x55; 32]),
            created_at: now - Duration::days(5),
            updated_at: now,
        }
    }

    #[test]
    fn public_certificate_detail_exposes_status_cert_pem_hex_fingerprint_and_never_key_pem() {
        let detail = build_public_certificate_detail(sample_record(false));
        let json = serde_json::to_value(&detail).expect("serialize public detail");
        let raw = serde_json::to_string(&detail).expect("serialize raw string");

        assert_eq!(json["status"], "valid");
        assert_eq!(
            json["cert_pem"],
            "-----BEGIN CERTIFICATE-----\nMIIB...\n-----END CERTIFICATE-----"
        );
        let fp = json["fingerprint"].as_str().expect("hex string fingerprint");
        assert_eq!(fp.len(), 64);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(fp, "55".repeat(32));
        assert!(
            json.get("key_pem").is_none(),
            "public certificate detail must never contain key_pem"
        );
        assert!(!raw.contains("encrypted-private-key-material-must-never-leak"));
    }

    #[test]
    fn summary_row_serializes_bytea_fingerprint_as_lowercase_hex_string() {
        let now = Utc::now();
        let summary = CertificateSummary::from(CertificateSummaryRow {
            id: Uuid::nil(),
            subject: "mazbri-auth".to_string(),
            issued_at: now,
            expires_at: now + Duration::days(90),
            revoked: false,
            revoked_at: None,
            renewal_count: 0,
            dns_names: vec!["mazbri-auth".to_string()],
            fingerprint: Some((0_u8..32_u8).collect()),
        });
        let json = serde_json::to_value(&summary).expect("serialize summary");
        let fp = json["fingerprint"].as_str().expect("hex string");
        assert_eq!(fp.len(), 64);
        assert_eq!(
            fp,
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
        );
    }

    #[test]
    fn active_subjects_repository_sql_filters_out_revoked_certificates() {
        let repo_src = include_str!("../../db/repository.rs");
        assert!(
            repo_src.contains(
                "SELECT DISTINCT subject FROM certificates WHERE revoked = FALSE ORDER BY subject"
            ),
            "list_active_subjects must filter revoked = FALSE and deduplicate ordered subjects"
        );
    }
}
