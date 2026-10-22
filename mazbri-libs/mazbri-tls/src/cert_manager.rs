use crate::{CertClient, TlsConfig, TlsError, TlsResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::oneshot;
use tracing::{info, warn};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedCertificate {
    pub id: uuid::Uuid,
    pub subject: String,
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_cert_pem: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl ManagedCertificate {
    #[must_use]
    pub fn needs_renewal(&self, threshold_days: i64) -> bool {
        let threshold = Utc::now() + chrono::Duration::days(threshold_days);
        self.expires_at < threshold
    }
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
    }
}
#[derive(Debug)]
pub struct CertManager {
    config: TlsConfig,
    client: CertClient,
    cert: Arc<RwLock<Option<ManagedCertificate>>>,
}
impl CertManager {
    #[must_use]
    pub fn new(config: TlsConfig) -> Self {
        let client = CertClient::new(&config.cert_service_url, &config.cert_service_token);
        Self {
            config,
            client,
            cert: Arc::new(RwLock::new(None)),
        }
    }
    pub async fn initialize(&self) -> TlsResult<ManagedCertificate> {
        info!(
            service_identity = % self.config.service_identity,
            "Fetching certificate from cert-service (centralized mode)"
        );
        let cert = self.fetch_from_cert_service().await?;
        if cert.needs_renewal(self.config.renewal_threshold_days) {
            info!(
                subject = % cert.subject, cert_id = % cert.id,
                "Certificate needs renewal, renewing..."
            );
            return self.renew_certificate(&cert).await;
        }
        Ok(cert)
    }
    pub async fn get_certificate(&self) -> TlsResult<ManagedCertificate> {
        let guard = self.cert.read().await;
        guard
            .clone()
            .ok_or_else(|| TlsError::CertificateNotFound(self.config.service_identity.clone()))
    }
    async fn fetch_from_cert_service(&self) -> TlsResult<ManagedCertificate> {
        let response = self
            .client
            .get_by_subject(&self.config.service_identity)
            .await?;
        let cert = ManagedCertificate {
            id: response.id,
            subject: response.subject,
            cert_pem: response.cert_pem,
            key_pem: response.key_pem,
            ca_cert_pem: response.ca_cert_pem,
            issued_at: response.issued_at,
            expires_at: response.expires_at,
            updated_at: Utc::now(),
        };
        *self.cert.write().await = Some(cert.clone());
        info!(
            subject = % cert.subject, cert_id = % cert.id, expires_at = % cert
            .expires_at, "Certificate fetched from cert-service and stored in memory"
        );
        Ok(cert)
    }
    async fn renew_certificate(
        &self,
        current: &ManagedCertificate,
    ) -> TlsResult<ManagedCertificate> {
        info!(
            subject = % current.subject, cert_id = % current.id, "Renewing certificate"
        );
        let response = self.client.renew(current.id).await?;
        let cert = ManagedCertificate {
            id: response.id,
            subject: response.subject,
            cert_pem: response.cert_pem,
            key_pem: response.key_pem,
            ca_cert_pem: response.ca_cert_pem,
            issued_at: response.issued_at,
            expires_at: response.expires_at,
            updated_at: Utc::now(),
        };
        *self.cert.write().await = Some(cert.clone());
        info!(
            subject = % cert.subject, cert_id = % cert.id, expires_at = % cert
            .expires_at, "Certificate renewed and stored in memory"
        );
        Ok(cert)
    }
    pub fn spawn_renewal_task(&self) -> (tokio::task::JoinHandle<()>, oneshot::Sender<()>) {
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let manager = Self {
            config: self.config.clone(),
            client: self.client.clone(),
            cert: Arc::clone(&self.cert),
        };
        let handle = tokio::spawn(async move {
            let check_interval = std::time::Duration::from_secs(3600);
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        info!("Renewal task received shutdown signal");
                        break;
                    }
                    _ = tokio::time::sleep(check_interval) => {
                        if let Some(cert) = manager.cert.read().await.clone() {
                            if cert.needs_renewal(manager.config.renewal_threshold_days) {
                                info!(
                                    subject = %cert.subject,
                                    "Certificate needs renewal, renewing in background"
                                );
                                match manager.renew_certificate(&cert).await {
                                    Ok(new_cert) => {
                                        info!(
                                            subject = %new_cert.subject,
                                            expires_at = %new_cert.expires_at,
                                            "Certificate renewed in background"
                                        );
                                    }
                                    Err(e) => {
                                        warn!(
                                            error = %e,
                                            "Failed to renew certificate in background"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            info!("Renewal task stopped");
        });
        (handle, shutdown_tx)
    }
    pub fn spawn_renewal_task_managed(&self) -> RenewalTaskHandle {
        let (handle, shutdown_tx) = self.spawn_renewal_task();
        RenewalTaskHandle {
            handle,
            shutdown_tx,
        }
    }
    pub async fn get_ca_cert(&self) -> TlsResult<String> {
        let cert = self.get_certificate().await?;
        Ok(cert.ca_cert_pem)
    }
}
#[derive(Clone)]
pub struct ManagedCertHandle {
    cert: Arc<RwLock<Option<ManagedCertificate>>>,
}
impl ManagedCertHandle {
    pub async fn check_expiry(&self, renewal_threshold_days: i64) -> CertExpiryInfo {
        let guard = self.cert.read().await;
        match guard.as_ref() {
            Some(cert) => {
                let now = Utc::now();
                let is_expired = cert.expires_at < now;
                let threshold = now + chrono::Duration::days(renewal_threshold_days);
                let is_near_expiry = cert.expires_at < threshold;
                CertExpiryInfo {
                    is_expired,
                    is_near_expiry,
                    expires_at: cert.expires_at,
                    subject: cert.subject.clone(),
                }
            }
            None => CertExpiryInfo {
                is_expired: true,
                is_near_expiry: true,
                expires_at: Utc::now(),
                subject: "no certificate".to_string(),
            },
        }
    }
}
#[derive(Debug, Clone)]
pub struct CertExpiryInfo {
    pub is_expired: bool,
    pub is_near_expiry: bool,
    pub expires_at: DateTime<Utc>,
    pub subject: String,
}
impl CertManager {
    #[must_use]
    pub fn cert_handle(&self) -> ManagedCertHandle {
        ManagedCertHandle {
            cert: Arc::clone(&self.cert),
        }
    }
}
pub struct RenewalTaskHandle {
    handle: tokio::task::JoinHandle<()>,
    shutdown_tx: oneshot::Sender<()>,
}
impl RenewalTaskHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(());
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(10), self.handle).await;
    }
}
