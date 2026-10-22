use chrono::Utc;
use mazbri_cert::license::LicenseDecision;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth::AuthState;
use crate::ca::{CaManager, CaStore};
use crate::config::{CertAuthConfig, CertConfig};
use crate::db::CertificateRepository;
use crate::error::{CertError, CertResult};

pub struct BaseAuthorizationState {
    repo: Arc<CertificateRepository>,
    auth_state: Arc<AuthState>,
    ca_manager: RwLock<Option<Arc<CaManager>>>,
    cert_config: RwLock<Option<Arc<CertConfig>>>,
}

impl BaseAuthorizationState {
    #[must_use]
    pub fn new(repo: Arc<CertificateRepository>, auth_state: Arc<AuthState>) -> Self {
        Self {
            repo,
            auth_state,
            ca_manager: RwLock::new(None),
            cert_config: RwLock::new(None),
        }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn require_issuance(&self) -> CertResult<Arc<CaManager>> {
        if crate::license_service::has_v3_state(self.repo.get_pool()).await? {
            let decision =
                crate::license_service::current_decision(self.repo.get_pool(), Utc::now()).await?;
            if !decision.issuance_allowed() {
                if decision.status == "grace" || decision.status == "expired" {
                    return Err(CertError::BaseAuthorizationExpired);
                }
                return Err(CertError::BaseAuthorizationNotActive);
            }
        }
        self.require_ca_manager().await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn activate(
        &self,
        cert_config: CertConfig,
        auth_config: CertAuthConfig,
    ) -> CertResult<Arc<CaManager>> {
        validate_runtime_config(&cert_config, &auth_config)?;
        let cert_config = Arc::new(cert_config);
        let ca_store = CaStore::new(self.repo.clone(), cert_config.clone());
        let ca_manager = Arc::new(ca_store.initialize().await?);
        self.auth_state.update_config(&auth_config).await;
        self.auth_state.token_manager.validate_config().await?;
        *self.cert_config.write().await = Some(cert_config);
        *self.ca_manager.write().await = Some(ca_manager.clone());
        Ok(ca_manager)
    }

    pub async fn deactivate(&self) {
        *self.ca_manager.write().await = None;
        *self.cert_config.write().await = None;
    }

    /// 配置中心写入的许可证闸，判定见 `management_writes_decision`。
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn require_management_writes(&self) -> CertResult<()> {
        let decision =
            crate::license_service::current_decision(self.repo.get_pool(), Utc::now()).await?;
        management_writes_decision(&decision)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn require_ca_manager(&self) -> CertResult<Arc<CaManager>> {
        self.ca_manager
            .read()
            .await
            .clone()
            .ok_or(CertError::BaseAuthorizationNotActive)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn require_cert_config(&self) -> CertResult<Arc<CertConfig>> {
        self.cert_config
            .read()
            .await
            .clone()
            .ok_or(CertError::BaseAuthorizationNotActive)
    }

    pub async fn is_active(&self) -> bool {
        self.ca_manager.read().await.is_some()
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn restore_active_package_if_needed(&self, pool: &PgPool) -> CertResult<bool> {
        if self.is_active().await {
            return Ok(true);
        }
        if !crate::license_service::has_v3_state(pool).await? {
            return Ok(false);
        }
        let static_tokens = self.auth_state.token_manager.static_tokens().await;
        crate::license_service::restore_v3(self, pool, &static_tokens).await
    }
}

/// 有效期和宽限期内允许改配置：宽限期只停新签发，运维还得能改配置。
/// 未导入、未生效、安装编号不匹配返回 `BaseAuthorizationNotActive`，
/// 过期返回 BaseAuthorizationExpired，两者都映射成 403。
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn management_writes_decision(decision: &LicenseDecision) -> CertResult<()> {
    match decision.status.as_str() {
        "active" | "grace" => Ok(()),
        "expired" => Err(CertError::BaseAuthorizationExpired),
        _ => Err(CertError::BaseAuthorizationNotActive),
    }
}

fn validate_runtime_config(cert: &CertConfig, auth: &CertAuthConfig) -> CertResult<()> {
    if cert
        .key_password
        .as_ref()
        .is_none_or(std::string::String::is_empty)
    {
        return Err(CertError::InvalidRequest(
            "key_password is required".to_string(),
        ));
    }
    if cert.ca_validity_days <= 0 || cert.cert_validity_days <= 0 {
        return Err(CertError::InvalidRequest(
            "certificate validity values must be positive".to_string(),
        ));
    }
    if cert.cert_validity_days >= cert.ca_validity_days {
        return Err(CertError::InvalidRequest(
            "cert_validity_days must be less than ca_validity_days".to_string(),
        ));
    }
    if auth.static_tokens.is_empty() {
        return Err(CertError::InvalidRequest(
            "static_tokens is required".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::management_writes_decision;
    use crate::error::CertError;
    use mazbri_cert::license::{LicenseDecision, unconfigured_decision};

    fn with_status(status: &str) -> LicenseDecision {
        let mut decision = unconfigured_decision();
        decision.status = status.to_string();
        decision
    }

    #[test]
    fn active_and_grace_allow_config_writes() {
        assert!(management_writes_decision(&with_status("active")).is_ok());
        assert!(management_writes_decision(&with_status("grace")).is_ok());
    }

    #[test]
    fn expired_license_blocks_config_writes() {
        assert!(matches!(
            management_writes_decision(&with_status("expired")),
            Err(CertError::BaseAuthorizationExpired)
        ));
    }

    #[test]
    fn missing_or_invalid_license_blocks_config_writes() {
        for status in ["unconfigured", "invalid", "pending"] {
            assert!(
                matches!(
                    management_writes_decision(&with_status(status)),
                    Err(CertError::BaseAuthorizationNotActive)
                ),
                "{status}"
            );
        }
    }

    #[test]
    fn unconfigured_decision_blocks_config_writes() {
        assert!(matches!(
            management_writes_decision(&unconfigured_decision()),
            Err(CertError::BaseAuthorizationNotActive)
        ));
    }
}
