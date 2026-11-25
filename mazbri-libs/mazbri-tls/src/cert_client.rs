use crate::{TlsError, TlsResult};
use chrono::{DateTime, Utc};
use mazbri_core::crypto::tenant_key::RootProtectionKeyring;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Debug, Clone)]
pub struct CertClient {
    client: Client,
    base_url: String,
    auth_token: Zeroizing<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CertificateResponse {
    pub id: Uuid,
    pub subject: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub cert_pem: String,
    #[serde(with = "crate::cert_manager::serde_zeroizing_string")]
    pub key_pem: Zeroizing<String>,
    pub ca_cert_pem: String,
    #[serde(default)]
    pub status: Option<String>,
}

impl std::fmt::Debug for CertificateResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertificateResponse")
            .field("id", &self.id)
            .field("subject", &self.subject)
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .field("key_pem", &"<redacted>")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokedSerialsResponse {
    pub digest: String,
    pub serials: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct IssueRequest {
    subject: String,
}

impl CertClient {
    #[must_use]
    pub fn new(base_url: &str, auth_token: &str) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_token: Zeroizing::new(auth_token.to_owned()),
        }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn issue(&self, subject: &str) -> TlsResult<CertificateResponse> {
        let url = format!("{}/issue", self.base_url);
        let request = IssueRequest {
            subject: subject.to_string(),
        };
        info!(
            subject = %subject, url = %url, "Requesting certificate from cert-service"
        );
        let response = self
            .client
            .post(&url)
            .json(&request)
            .bearer_auth(self.auth_token.as_str())
            .send()
            .await?;
        let status = response.status();
        debug!(status = %status, "Received response from cert-service");
        if status.is_success() {
            let cert_response: CertificateResponse = response.json().await?;
            info!(
                subject = %cert_response.subject, cert_id = %cert_response.id,
                expires_at = %cert_response.expires_at,
                "Certificate issued successfully"
            );
            Ok(cert_response)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            warn!(
                status = %status, error = %error_text, "Failed to issue certificate"
            );
            Err(TlsError::IssueFailed(format!(
                "HTTP {status}: {error_text}"
            )))
        }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn renew(&self, cert_id: Uuid) -> TlsResult<CertificateResponse> {
        let url = format!("{}/renew/{}", self.base_url, cert_id);
        info!(cert_id = %cert_id, url = %url, "Requesting certificate renewal");
        let response = self
            .client
            .post(&url)
            .bearer_auth(self.auth_token.as_str())
            .send()
            .await?;
        let status = response.status();
        debug!(status = %status, "Received renewal response from cert-service");
        if status.is_success() {
            let cert_response: CertificateResponse = response.json().await?;
            info!(
                subject = %cert_response.subject, cert_id = %cert_response.id,
                expires_at = %cert_response.expires_at,
                "Certificate renewed successfully"
            );
            Ok(cert_response)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            warn!(
                status = %status, error = %error_text, "Failed to renew certificate"
            );
            Err(TlsError::RenewFailed(format!(
                "HTTP {status}: {error_text}"
            )))
        }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_by_subject(&self, subject: &str) -> TlsResult<CertificateResponse> {
        let url = format!(
            "{}/certificates/by-subject/{}",
            self.base_url,
            urlencoding::encode(subject)
        );
        info!(
            subject = %subject, url = %url,
            "Fetching certificate by subject from cert-service"
        );
        let response = self
            .client
            .get(&url)
            .bearer_auth(self.auth_token.as_str())
            .send()
            .await?;
        let status = response.status();
        debug!(status = %status, "Received certificate response from cert-service");
        if status.is_success() {
            let cert_response: CertificateResponse = response.json().await?;
            info!(
                subject = %cert_response.subject, cert_id = %cert_response.id,
                expires_at = %cert_response.expires_at, status = ?cert_response.status,
                "Certificate fetched successfully"
            );
            Ok(cert_response)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            warn!(
                status = %status, error = %error_text,
                "Failed to fetch certificate by subject"
            );
            Err(TlsError::CertificateNotFound(format!(
                "{subject}: HTTP {status} - {error_text}"
            )))
        }
    }

    /// 拉取已吊销证书序列号集合与摘要。
    ///
    /// # Errors
    ///
    /// HTTP 请求或 JSON 解析失败时返回错误。
    pub async fn get_revoked_serials(&self) -> TlsResult<RevokedSerialsResponse> {
        let url = format!("{}/certificates/revoked-serials", self.base_url);
        let response = self
            .client
            .get(&url)
            .bearer_auth(self.auth_token.as_str())
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            Ok(response.json().await?)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(TlsError::Config(format!(
                "Failed to fetch revoked serials (HTTP {status}): {error_text}"
            )))
        }
    }

    /// 拉取根材料多版本密钥环（供 `Gateway` / `Bypass` / `Mgmt` 使用）。
    ///
    /// # Errors
    ///
    /// HTTP 请求或 JSON 解析失败时返回错误。
    pub async fn get_root_material_keyring(&self) -> TlsResult<RootProtectionKeyring> {
        mazbri_core::crypto::tenant_key::fetch_root_protection_keyring(
            &self.client,
            &self.base_url,
            self.auth_token.as_str(),
        )
        .await
        .map_err(|e| TlsError::Config(e.to_string()))
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn health_check(&self) -> TlsResult<bool> {
        let url = format!("{}/health", self.base_url);
        debug!(url = %url, "Performing health check on cert-service");
        let response = self.client.get(&url).send().await?;
        Ok(response.status().is_success())
    }
}
