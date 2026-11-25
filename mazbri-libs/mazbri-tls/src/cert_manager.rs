use crate::transport::{build_certified_key, normalize_serial_bytes};
use crate::{CertClient, TlsConfig, TlsError, TlsResult};
use arc_swap::{ArcSwap, ArcSwapOption};
use chrono::{DateTime, TimeDelta, Utc};
use rustls::sign::CertifiedKey;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::RwLock;
use tokio::sync::oneshot;
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

const NO_CERT_EXPIRES_AT: i64 = i64::MIN;

pub(crate) mod serde_zeroizing_string {
    use serde::{Deserialize, Deserializer, Serializer};
    use zeroize::Zeroizing;

    pub fn serialize<S>(value: &Zeroizing<String>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(value.as_str())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Zeroizing<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Zeroizing::new(s))
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ManagedCertificate {
    pub id: uuid::Uuid,
    pub subject: String,
    pub cert_pem: String,
    #[serde(with = "serde_zeroizing_string")]
    pub key_pem: Zeroizing<String>,
    pub ca_cert_pem: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl std::fmt::Debug for ManagedCertificate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedCertificate")
            .field("id", &self.id)
            .field("subject", &self.subject)
            .field("key_pem", &"<redacted>")
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .field("updated_at", &self.updated_at)
            .finish_non_exhaustive()
    }
}

impl ManagedCertificate {
    #[must_use]
    pub fn needs_renewal(&self, threshold_days: i64) -> bool {
        let delta = TimeDelta::try_days(threshold_days).unwrap_or_else(TimeDelta::zero);
        let threshold = Utc::now() + delta;
        self.expires_at < threshold
    }

    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
    }
}

pub struct CertManager {
    config: TlsConfig,
    client: CertClient,
    cert: Arc<RwLock<Option<ManagedCertificate>>>,
    certified_key: Arc<ArcSwapOption<CertifiedKey>>,
    ca_cert_pem: Arc<ArcSwapOption<String>>,
    revoked_serials: Arc<ArcSwap<HashSet<Vec<u8>>>>,
    revoked_digest: Arc<Mutex<String>>,
    expires_at_unix: Arc<AtomicI64>,
}

impl std::fmt::Debug for CertManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertManager")
            .field("service_identity", &self.config.service_identity)
            .field("has_certified_key", &self.certified_key.load().is_some())
            .finish_non_exhaustive()
    }
}

impl CertManager {
    #[must_use]
    pub fn new(config: TlsConfig) -> Self {
        let client = CertClient::new(&config.cert_service_url, &config.cert_service_token);
        Self {
            config,
            client,
            cert: Arc::new(RwLock::new(None)),
            certified_key: Arc::new(ArcSwapOption::from(None)),
            ca_cert_pem: Arc::new(ArcSwapOption::from(None)),
            revoked_serials: Arc::new(ArcSwap::from_pointee(HashSet::new())),
            revoked_digest: Arc::new(Mutex::new(String::new())),
            expires_at_unix: Arc::new(AtomicI64::new(NO_CERT_EXPIRES_AT)),
        }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn initialize(&self) -> TlsResult<ManagedCertificate> {
        info!(
            service_identity = %self.config.service_identity,
            "Fetching certificate from cert-service (centralized mode)"
        );
        let cert = self.fetch_from_cert_service().await?;
        let _ = self.refresh_revoked_serials().await;
        if cert.needs_renewal(self.config.renewal_threshold_days) {
            info!(
                subject = %cert.subject, cert_id = %cert.id,
                "Certificate needs renewal, renewing..."
            );
            return self.renew_certificate(&cert).await;
        }
        Ok(cert)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_certificate(&self) -> TlsResult<ManagedCertificate> {
        let guard = self.cert.read().await;
        guard
            .clone()
            .ok_or_else(|| TlsError::CertificateNotFound(self.config.service_identity.clone()))
    }

    async fn store_active_certificate(&self, cert: ManagedCertificate) -> TlsResult<()> {
        let certified = build_certified_key(&cert.cert_pem, &cert.key_pem)?;
        let expires_ts = cert.expires_at.timestamp();
        let ca_pem = cert.ca_cert_pem.clone();
        self.certified_key.store(Some(certified));
        self.ca_cert_pem.store(Some(Arc::new(ca_pem)));
        *self.cert.write().await = Some(cert);
        self.expires_at_unix.store(expires_ts, Ordering::Release);
        Ok(())
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
        self.store_active_certificate(cert.clone()).await?;
        info!(
            subject = %cert.subject, cert_id = %cert.id, expires_at = %cert.expires_at,
            "Certificate fetched from cert-service and atomically activated"
        );
        Ok(cert)
    }

    async fn renew_certificate(
        &self,
        current: &ManagedCertificate,
    ) -> TlsResult<ManagedCertificate> {
        info!(
            subject = %current.subject, cert_id = %current.id, "Renewing certificate"
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
        self.store_active_certificate(cert.clone()).await?;
        info!(
            subject = %cert.subject, cert_id = %cert.id, expires_at = %cert.expires_at,
            "Certificate renewed and atomically swapped in memory"
        );
        Ok(cert)
    }

    /// 从 `cert-service` 刷新已吊销序列号缓存。
    ///
    /// # Errors
    ///
    /// 网络请求或响应解析失败时返回错误。
    pub async fn refresh_revoked_serials(&self) -> TlsResult<bool> {
        self.cert_handle()
            .refresh_revoked_serials(&self.client)
            .await
    }

    #[must_use]
    pub fn spawn_renewal_task(&self) -> (tokio::task::JoinHandle<()>, oneshot::Sender<()>) {
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let manager = Self {
            config: self.config.clone(),
            client: self.client.clone(),
            cert: Arc::clone(&self.cert),
            certified_key: Arc::clone(&self.certified_key),
            ca_cert_pem: Arc::clone(&self.ca_cert_pem),
            revoked_serials: Arc::clone(&self.revoked_serials),
            revoked_digest: Arc::clone(&self.revoked_digest),
            expires_at_unix: Arc::clone(&self.expires_at_unix),
        };
        let handle = tokio::spawn(async move {
            let poll_interval = std::time::Duration::from_secs(30);
            let mut ticks: u64 = 0;
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        info!("Renewal task received shutdown signal");
                        break;
                    }
                    () = tokio::time::sleep(poll_interval) => {
                        ticks = ticks.wrapping_add(1);
                        if !manager.config.cert_service_url.trim().is_empty()
                            && let Err(e) = manager.refresh_revoked_serials().await
                        {
                            debug!(error = %e, "Background revoked serials sync skipped");
                        }
                        // 每 60 个周期（30 分钟）或证书已进入续期窗口时执行续期检查。
                        let should_check_renewal = ticks.is_multiple_of(60)
                            || manager.cert_handle().is_near_expiry_now(manager.config.renewal_threshold_days);
                        if should_check_renewal
                            && let Some(cert) = manager.cert.read().await.clone()
                            && cert.needs_renewal(manager.config.renewal_threshold_days)
                        {
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
            info!("Renewal task stopped");
        });
        (handle, shutdown_tx)
    }

    #[must_use]
    pub fn spawn_renewal_task_managed(&self) -> RenewalTaskHandle {
        let (handle, shutdown_tx) = self.spawn_renewal_task();
        RenewalTaskHandle {
            handle,
            shutdown_tx,
        }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_ca_cert(&self) -> TlsResult<String> {
        let cert = self.get_certificate().await?;
        Ok(cert.ca_cert_pem)
    }

    #[must_use]
    pub fn cert_handle(&self) -> ManagedCertHandle {
        ManagedCertHandle {
            cert: Arc::clone(&self.cert),
            certified_key: Arc::clone(&self.certified_key),
            ca_cert_pem: Arc::clone(&self.ca_cert_pem),
            revoked_serials: Arc::clone(&self.revoked_serials),
            revoked_digest: Arc::clone(&self.revoked_digest),
            expires_at_unix: Arc::clone(&self.expires_at_unix),
        }
    }
}

#[derive(Clone)]
pub struct ManagedCertHandle {
    cert: Arc<RwLock<Option<ManagedCertificate>>>,
    certified_key: Arc<ArcSwapOption<CertifiedKey>>,
    ca_cert_pem: Arc<ArcSwapOption<String>>,
    revoked_serials: Arc<ArcSwap<HashSet<Vec<u8>>>>,
    revoked_digest: Arc<Mutex<String>>,
    expires_at_unix: Arc<AtomicI64>,
}

impl std::fmt::Debug for ManagedCertHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedCertHandle")
            .field("has_certified_key", &self.certified_key.load().is_some())
            .field("expires_at_unix", &self.expires_at_unix())
            .finish_non_exhaustive()
    }
}

impl ManagedCertHandle {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            cert: Arc::new(RwLock::new(None)),
            certified_key: Arc::new(ArcSwapOption::from(None)),
            ca_cert_pem: Arc::new(ArcSwapOption::from(None)),
            revoked_serials: Arc::new(ArcSwap::from_pointee(HashSet::new())),
            revoked_digest: Arc::new(Mutex::new(String::new())),
            expires_at_unix: Arc::new(AtomicI64::new(NO_CERT_EXPIRES_AT)),
        }
    }

    #[must_use]
    pub fn from_certificate(cert: ManagedCertificate) -> Self {
        let ts = cert.expires_at.timestamp();
        let ck = if !cert.cert_pem.trim().is_empty() && !cert.key_pem.trim().is_empty() {
            build_certified_key(&cert.cert_pem, &cert.key_pem).ok()
        } else {
            None
        };
        let ca = (!cert.ca_cert_pem.trim().is_empty()).then(|| Arc::new(cert.ca_cert_pem.clone()));
        Self {
            cert: Arc::new(RwLock::new(Some(cert))),
            certified_key: Arc::new(ArcSwapOption::from(ck)),
            ca_cert_pem: Arc::new(ArcSwapOption::from(ca)),
            revoked_serials: Arc::new(ArcSwap::from_pointee(HashSet::new())),
            revoked_digest: Arc::new(Mutex::new(String::new())),
            expires_at_unix: Arc::new(AtomicI64::new(ts)),
        }
    }

    /// 原子热替换当前进程的证书与私钥，新连接立即生效，存量连接不受影响。
    ///
    /// # Errors
    ///
    /// 证书或私钥 PEM 非法时返回错误（旧证书保持不变）。
    pub async fn update_certificate(&self, cert: ManagedCertificate) -> TlsResult<()> {
        let ck = build_certified_key(&cert.cert_pem, &cert.key_pem)?;
        let ts = cert.expires_at.timestamp();
        let ca = (!cert.ca_cert_pem.trim().is_empty()).then(|| Arc::new(cert.ca_cert_pem.clone()));
        self.certified_key.store(Some(ck));
        if ca.is_some() {
            self.ca_cert_pem.store(ca);
        }
        *self.cert.write().await = Some(cert);
        self.expires_at_unix.store(ts, Ordering::Release);
        Ok(())
    }

    pub(crate) fn certified_key_swap(&self) -> Arc<ArcSwapOption<CertifiedKey>> {
        Arc::clone(&self.certified_key)
    }

    pub(crate) fn ca_cert_pem(&self) -> Option<Arc<String>> {
        self.ca_cert_pem.load_full()
    }

    pub(crate) fn revoked_serials_swap(&self) -> Arc<ArcSwap<HashSet<Vec<u8>>>> {
        Arc::clone(&self.revoked_serials)
    }

    /// 直接替换本地已吊销证书序列号集合（字节序已规范化）。
    pub fn set_revoked_serials(&self, serials: HashSet<Vec<u8>>) {
        let normalized = serials
            .into_iter()
            .map(|s| normalize_serial_bytes(&s))
            .collect();
        self.revoked_serials.store(Arc::new(normalized));
    }

    /// 当心跳返回的 `expected_digest` 与本地不一致时拉取最新已吊销序列号。
    ///
    /// # Errors
    ///
    /// 网络或十六进制序列号解析失败时返回错误。
    pub async fn sync_revoked_serials_if_digest_changed(
        &self,
        client: &CertClient,
        expected_digest: &str,
    ) -> TlsResult<bool> {
        if expected_digest.is_empty() {
            return Ok(false);
        }
        {
            let guard = self
                .revoked_digest
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if guard.as_str() == expected_digest {
                return Ok(false);
            }
        }
        self.refresh_revoked_serials(client).await
    }

    /// 从 `cert-service` 拉取并原子更新已吊销序列号集合。
    ///
    /// # Errors
    ///
    /// 网络或响应解析失败时返回错误。
    pub async fn refresh_revoked_serials(&self, client: &CertClient) -> TlsResult<bool> {
        let resp = client.get_revoked_serials().await?;
        {
            let guard = self
                .revoked_digest
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if guard.as_str() == resp.digest {
                return Ok(false);
            }
        }
        let mut set = HashSet::with_capacity(resp.serials.len());
        for hex_serial in &resp.serials {
            if let Some(raw) = decode_hex_bytes(hex_serial.trim()) {
                set.insert(normalize_serial_bytes(&raw));
            }
        }
        self.revoked_serials.store(Arc::new(set));
        *self
            .revoked_digest
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = resp.digest;
        Ok(true)
    }

    /// 读取当前句柄中最新的证书快照（后台续期后自动返回新证书）。
    pub async fn current_certificate(&self) -> Option<ManagedCertificate> {
        self.cert.read().await.clone()
    }

    #[must_use]
    pub fn expires_at_unix(&self) -> Option<i64> {
        let ts = self.expires_at_unix.load(Ordering::Acquire);
        (ts != NO_CERT_EXPIRES_AT).then_some(ts)
    }

    #[must_use]
    pub fn cert_expiry(&self) -> Option<DateTime<Utc>> {
        self.expires_at_unix()
            .and_then(|ts| DateTime::from_timestamp(ts, 0))
    }

    #[must_use]
    pub fn is_expired_now(&self) -> bool {
        self.cert_expiry().is_none_or(|exp| exp < Utc::now())
    }

    #[must_use]
    pub fn is_near_expiry_now(&self, renewal_threshold_days: i64) -> bool {
        let Some(exp) = self.cert_expiry() else {
            return true;
        };
        let delta = TimeDelta::try_days(renewal_threshold_days).unwrap_or_else(TimeDelta::zero);
        exp < Utc::now() + delta
    }

    #[must_use]
    pub fn check_expiry_lockfree(&self, renewal_threshold_days: i64) -> CertExpiryInfo {
        let now = Utc::now();
        let Some(expires_at) = self.cert_expiry() else {
            return CertExpiryInfo {
                is_expired: true,
                is_near_expiry: true,
                expires_at: now,
                subject: "no certificate".to_string(),
            };
        };
        let delta = TimeDelta::try_days(renewal_threshold_days).unwrap_or_else(TimeDelta::zero);
        let subject = self
            .cert
            .try_read()
            .ok()
            .and_then(|guard| guard.as_ref().map(|c| c.subject.clone()))
            .unwrap_or_else(|| "managed certificate".to_string());
        CertExpiryInfo {
            is_expired: expires_at < now,
            is_near_expiry: expires_at < now + delta,
            expires_at,
            subject,
        }
    }

    pub async fn check_expiry(&self, renewal_threshold_days: i64) -> CertExpiryInfo {
        let guard = self.cert.read().await;
        match guard.as_ref() {
            Some(cert) => {
                let now = Utc::now();
                let is_expired = cert.expires_at < now;
                let delta =
                    TimeDelta::try_days(renewal_threshold_days).unwrap_or_else(TimeDelta::zero);
                let threshold = now + delta;
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

pub struct RenewalTaskHandle {
    handle: tokio::task::JoinHandle<()>,
    shutdown_tx: oneshot::Sender<()>,
}

fn decode_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_hex_bytes(hex_str: &str) -> Option<Vec<u8>> {
    let bytes = hex_str.as_bytes();
    if bytes.is_empty() || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.as_chunks::<2>().0 {
        let hi = decode_hex_nibble(pair[0])?;
        let lo = decode_hex_nibble(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

impl RenewalTaskHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(());
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(10), self.handle).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{ManagedCertHandle, ManagedCertificate};
    use chrono::{TimeDelta, Utc};
    use zeroize::Zeroizing;

    fn sample_cert(expires_offset_days: i64) -> ManagedCertificate {
        let now = Utc::now();
        ManagedCertificate {
            id: uuid::Uuid::now_v7(),
            subject: "mazbri-gateway".to_string(),
            cert_pem: String::new(),
            key_pem: Zeroizing::new(String::new()),
            ca_cert_pem: String::new(),
            issued_at: now - TimeDelta::days(1),
            expires_at: now
                + TimeDelta::try_days(expires_offset_days).unwrap_or_else(TimeDelta::zero),
            updated_at: now,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn lockfree_expiry_reads_work_on_current_thread_runtime() {
        let empty = ManagedCertHandle::empty();
        assert!(empty.expires_at_unix().is_none());
        assert!(empty.is_expired_now());
        assert!(empty.is_near_expiry_now(30));

        let near = ManagedCertHandle::from_certificate(sample_cert(15));
        assert!(near.expires_at_unix().is_some());
        assert!(!near.is_expired_now());
        assert!(near.is_near_expiry_now(30));
        let info = near.check_expiry_lockfree(30);
        assert!(!info.is_expired);
        assert!(info.is_near_expiry);

        let expired = ManagedCertHandle::from_certificate(sample_cert(-1));
        assert!(expired.is_expired_now());
        assert!(expired.is_near_expiry_now(30));
    }

    #[test]
    fn managed_certificate_debug_redacts_private_key() {
        let mut cert = sample_cert(30);
        cert.key_pem = Zeroizing::new("SUPER-SECRET-PRIVATE-KEY".to_string());
        let rendered = format!("{cert:?}");
        assert!(!rendered.contains("SUPER-SECRET-PRIVATE-KEY"));
        assert!(rendered.contains("<redacted>"));
    }
}
