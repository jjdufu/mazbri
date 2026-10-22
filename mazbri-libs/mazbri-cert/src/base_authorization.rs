use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseAuthorizationSummary {
    pub package_id: Option<uuid::Uuid>,
    pub package_version: Option<String>,
    pub package_format: Option<String>,
    pub purpose: Option<String>,
    pub package_sha256: Option<String>,
    pub status: String,
    pub active: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
    pub seconds_to_expiry: Option<i64>,
    pub renewable: bool,
    pub renewed_from_package_id: Option<uuid::Uuid>,
    pub superseded_by_package_id: Option<uuid::Uuid>,
    pub created_by: Option<String>,
    pub imported_by: Option<uuid::Uuid>,
    pub created_at: Option<DateTime<Utc>>,
    pub imported_at: Option<DateTime<Utc>>,
    pub activated_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

#[must_use]
pub fn is_package_expired(
    now: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    marked_expired_at: Option<DateTime<Utc>>,
) -> bool {
    marked_expired_at.is_some() || expires_at.is_some_and(|deadline| deadline <= now)
}

#[must_use]
pub fn derive_base_authorization_status(status: &str, expired: bool) -> String {
    if expired && status != "revoked" {
        "expired".to_string()
    } else {
        status.to_string()
    }
}
