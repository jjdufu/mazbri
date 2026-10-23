use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct CertificateRecord {
    pub id: Uuid,
    pub subject: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
    pub revoked_at: Option<DateTime<Utc>>,
    pub cert_pem: String,
    pub key_pem: String,
    #[serde(default)]
    pub dns_names: Vec<String>,
    pub created_by_ip: Option<std::net::IpAddr>,
    pub renewal_count: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<Vec<u8>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueCertificateRequest {
    pub subject: String,
    #[serde(default = "default_validity_days")]
    pub validity_days: i64,
}
fn default_validity_days() -> i64 {
    365
}
#[derive(Debug, Clone, Serialize)]
pub struct CertificateResponse {
    pub id: Uuid,
    pub subject: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_cert_pem: String,
    #[serde(default)]
    pub dns_names: Vec<String>,
}
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CertificateSummary {
    pub id: Uuid,
    pub subject: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
    pub revoked_at: Option<DateTime<Utc>>,
    pub renewal_count: i32,
    #[serde(default)]
    pub dns_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<Vec<u8>>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyCertificateRequest {
    pub cert_pem: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct VerifyCertificateResponse {
    pub valid: bool,
    pub subject: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked: Option<bool>,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matches: Option<Vec<CertificateMatchSummary>>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CertificateMatchSummary {
    pub id: Uuid,
    pub subject: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
    #[serde(default)]
    pub dns_names: Vec<String>,
}
#[derive(Debug, Clone, FromRow)]
pub struct AuditLog {
    pub id: Uuid,
    pub action: String,
    pub actor_ip: Option<String>,
    pub target_subject: Option<String>,
    pub target_cert_id: Option<Uuid>,
    pub result: String,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Clone, FromRow)]
pub struct CaStorageRecord {
    pub id: Uuid,
    pub ca_cert_pem: String,
    pub ca_key_encrypted: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_active: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct FullCertificateResponse {
    pub id: Uuid,
    pub subject: String,
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_cert_pem: String,
    #[serde(default)]
    pub dns_names: Vec<String>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CertificateStatus {
    Valid,
    ExpiringSoon,
    Expired,
    Revoked,
}
impl CertificateStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::ExpiringSoon => "expiring_soon",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
        }
    }
}
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoRenewalConfig {
    pub id: Uuid,
    pub certificate_id: Uuid,
    pub enabled: bool,
    pub threshold_days: i32,
    pub validity_days: i32,
    pub last_renewal_at: Option<DateTime<Utc>>,
    pub next_renewal_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigureAutoRenewalRequest {
    pub enabled: bool,
    #[serde(default = "default_threshold_days")]
    pub threshold_days: i32,
    #[serde(default = "default_validity_days_config")]
    pub validity_days: i32,
}
fn default_threshold_days() -> i32 {
    30
}
fn default_validity_days_config() -> i32 {
    365
}
#[derive(Debug, Clone, Serialize)]
pub struct AutoRenewalConfigResponse {
    pub certificate_id: Uuid,
    pub enabled: bool,
    pub threshold_days: i32,
    pub validity_days: i32,
    pub last_renewal_at: Option<DateTime<Utc>>,
    pub next_renewal_at: Option<DateTime<Utc>>,
    pub certificate_expires_at: DateTime<Utc>,
}
