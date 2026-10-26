use crate::error::ApiError;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatus {
    pub status: String,
    pub issuance_allowed: bool,
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

    #[test]
    fn issuance_gate_follows_issuance_allowed() {
        let blocked = LicenseStatus {
            status: "grace".to_string(),
            issuance_allowed: false,
        };
        assert!(ensure_license_issuance(&blocked).is_err());
        let open = LicenseStatus {
            status: "active".to_string(),
            issuance_allowed: true,
        };
        assert!(ensure_license_issuance(&open).is_ok());
    }
}
