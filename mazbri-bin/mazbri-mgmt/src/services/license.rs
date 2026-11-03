use crate::error::ApiError;
use serde::{Deserialize, Serialize};

/// 许可证状态（与 cert-service `decision_json` 对齐；业务可读，不按角色裁剪）。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatus {
    pub status: String,
    #[serde(default)]
    pub active: bool,
    pub issuance_allowed: bool,
    #[serde(default)]
    pub decryption_allowed: bool,
    #[serde(default)]
    pub sequence: Option<i64>,
    #[serde(default)]
    pub key_version: Option<String>,
    #[serde(default)]
    pub customer_id: Option<String>,
    #[serde(default)]
    pub installation_id: Option<String>,
    #[serde(default)]
    pub not_before: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub grace_until: Option<String>,
    #[serde(default)]
    pub seconds_to_expiry: Option<i64>,
    #[serde(default)]
    pub seconds_to_grace_end: Option<i64>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn fetch_license_status(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
) -> Result<LicenseStatus, ApiError> {
    let url = format!(
        "{}/authorizations/license/status",
        base_url.trim_end_matches('/')
    );
    let request = client
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .bearer_auth(token);
    let response = crate::audit::actor_header::send_to_cert(request)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to query license status: {error}")))?;
    if !response.status().is_success() {
        return Err(ApiError::internal(format!(
            "License status request failed with {}",
            response.status()
        )));
    }
    response
        .json::<LicenseStatus>()
        .await
        .map_err(|error| ApiError::internal(format!("Failed to parse license status: {error}")))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn ensure_license_issuance(status: &LicenseStatus) -> Result<(), ApiError> {
    if status.issuance_allowed {
        Ok(())
    } else {
        Err(ApiError::license_issuance_not_allowed())
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn ensure_license_issuance_allowed(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
) -> Result<(), ApiError> {
    let status = fetch_license_status(client, base_url, token).await?;
    ensure_license_issuance(&status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(status: &str, issuance_allowed: bool) -> LicenseStatus {
        LicenseStatus {
            status: status.to_string(),
            active: issuance_allowed,
            issuance_allowed,
            decryption_allowed: false,
            sequence: None,
            key_version: None,
            customer_id: None,
            installation_id: None,
            not_before: None,
            expires_at: None,
            grace_until: None,
            seconds_to_expiry: None,
            seconds_to_grace_end: None,
        }
    }

    #[test]
    fn issuance_gate_follows_issuance_allowed() {
        assert!(ensure_license_issuance(&status("grace", false)).is_err());
        assert!(ensure_license_issuance(&status("active", true)).is_ok());
    }

    #[test]
    fn status_field_is_stable_for_ui() {
        let raw = serde_json::json!({
            "status": "unconfigured",
            "active": false,
            "issuanceAllowed": false,
            "decryptionAllowed": false,
            "sequence": null,
            "keyVersion": null,
            "customerId": null,
            "installationId": null,
            "notBefore": null,
            "expiresAt": null,
            "graceUntil": null,
            "secondsToExpiry": null,
            "secondsToGraceEnd": null
        });
        let parsed: LicenseStatus = serde_json::from_value(raw).expect("parse");
        assert_eq!(parsed.status, "unconfigured");
        assert!(!parsed.issuance_allowed);
        let encoded = serde_json::to_value(&parsed).expect("encode");
        assert_eq!(encoded["status"], "unconfigured");
        assert_eq!(encoded["issuanceAllowed"], false);
    }
}
