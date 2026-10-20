use crate::ca::CaManager;
use crate::config::CertConfig;
use crate::db::repository::CertificateRepository;
use crate::error::CertResult;
use std::sync::Arc;
use tracing::info;
pub struct CaStore {
    repo: Arc<CertificateRepository>,
    config: Arc<CertConfig>,
}
impl CaStore {
    #[must_use]
    pub fn new(repo: Arc<CertificateRepository>, config: Arc<CertConfig>) -> Self {
        Self { repo, config }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn initialize(&self) -> CertResult<CaManager> {
        if let Some(ca_record) = self.repo.get_active_ca().await? {
            info!("从数据库加载现有 CA");
            return CaManager::from_existing(
                &ca_record.ca_cert_pem,
                &ca_record.ca_key_encrypted,
                self.config.clone(),
            );
        }
        info!("生成新 CA");
        let manager = CaManager::new(self.config.clone())?;
        let ca_cert_pem = manager.get_ca_pem().clone();
        let ca_key_encrypted = manager.get_ca_key_encrypted()?;
        self.repo.store_ca(&ca_cert_pem, &ca_key_encrypted).await?;
        info!("CA 已存储到数据库");
        Ok(manager)
    }
}
