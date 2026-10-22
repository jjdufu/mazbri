use crate::{TlsError, TlsResult};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;
#[derive(Debug, Clone)]
pub struct CertClient {
    client: Client,
    base_url: String,
    auth_token: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateResponse {
    pub id: Uuid,
    pub subject: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_cert_pem: String,
    #[serde(default)]
    pub status: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
struct IssueRequest {
    subject: String,
}
impl CertClient {
    #[must_use]
    pub fn new(base_url: &str, auth_token: Option<&str>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_token: auth_token.map(std::string::ToString::to_string),
        }
    }
    pub async fn issue(&self, subject: &str) -> TlsResult<CertificateResponse> {
        let url = format!("{}/issue", self.base_url);
        let request = IssueRequest {
            subject: subject.to_string(),
        };
        info!(
            subject = % subject, url = % url, "Requesting certificate from cert-service"
        );
        let mut builder = self.client.post(&url).json(&request);
        if let Some(ref token) = self.auth_token {
            builder = builder.bearer_auth(token);
        }
        let response = builder.send().await?;
        let status = response.status();
        debug!(status = % status, "Received response from cert-service");
        if status.is_success() {
            let cert_response: CertificateResponse = response.json().await?;
            info!(
                subject = % cert_response.subject, cert_id = % cert_response.id,
                expires_at = % cert_response.expires_at,
                "Certificate issued successfully"
            );
            Ok(cert_response)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            warn!(
                status = % status, error = % error_text, "Failed to issue certificate"
            );
            Err(TlsError::IssueFailed(format!(
                "HTTP {}: {}",
                status, error_text
            )))
        }
    }
    pub async fn renew(&self, cert_id: Uuid) -> TlsResult<CertificateResponse> {
        let url = format!("{}/renew/{}", self.base_url, cert_id);
        info!(cert_id = % cert_id, url = % url, "Requesting certificate renewal");
        let mut builder = self.client.post(&url);
        if let Some(ref token) = self.auth_token {
            builder = builder.bearer_auth(token);
        }
        let response = builder.send().await?;
        let status = response.status();
        debug!(status = % status, "Received renewal response from cert-service");
        if status.is_success() {
            let cert_response: CertificateResponse = response.json().await?;
            info!(
                subject = % cert_response.subject, cert_id = % cert_response.id,
                expires_at = % cert_response.expires_at,
                "Certificate renewed successfully"
            );
            Ok(cert_response)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            warn!(
                status = % status, error = % error_text, "Failed to renew certificate"
            );
            Err(TlsError::RenewFailed(format!(
                "HTTP {}: {}",
                status, error_text
            )))
        }
    }
    pub async fn get_by_subject(&self, subject: &str) -> TlsResult<CertificateResponse> {
        let url = format!(
            "{}/certificates/by-subject/{}",
            self.base_url,
            urlencoding::encode(subject)
        );
        info!(
            subject = % subject, url = % url,
            "Fetching certificate by subject from cert-service"
        );
        let mut builder = self.client.get(&url);
        if let Some(ref token) = self.auth_token {
            builder = builder.bearer_auth(token);
        }
        let response = builder.send().await?;
        let status = response.status();
        debug!(status = % status, "Received certificate response from cert-service");
        if status.is_success() {
            let cert_response: CertificateResponse = response.json().await?;
            info!(
                subject = % cert_response.subject, cert_id = % cert_response.id,
                expires_at = % cert_response.expires_at, status = ? cert_response.status,
                "Certificate fetched successfully"
            );
            Ok(cert_response)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            warn!(
                status = % status, error = % error_text,
                "Failed to fetch certificate by subject"
            );
            Err(TlsError::CertificateNotFound(format!(
                "{}: HTTP {} - {}",
                subject, status, error_text
            )))
        }
    }
    pub async fn health_check(&self) -> TlsResult<bool> {
        let url = format!("{}/health", self.base_url);
        debug!(url = % url, "Performing health check on cert-service");
        let response = self.client.get(&url).send().await?;
        Ok(response.status().is_success())
    }
}
