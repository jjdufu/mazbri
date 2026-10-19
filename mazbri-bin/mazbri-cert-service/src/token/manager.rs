use crate::config::CertAuthConfig;
use crate::error::CertResult;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;
use tracing::info;
pub struct TokenManager {
    config: RwLock<Arc<CertAuthConfig>>,
}
impl TokenManager {
    pub fn new(config: Arc<CertAuthConfig>) -> Self {
        Self {
            config: RwLock::new(config),
        }
    }
    pub async fn update_config(&self, config: Arc<CertAuthConfig>) {
        *self.config.write().await = config;
    }
    pub async fn validate(&self, token: &str) -> CertResult<bool> {
        let config = self.config.read().await;
        Ok(config
            .static_tokens
            .iter()
            .any(|t| t.as_bytes().ct_eq(token.as_bytes()).into()))
    }
    pub async fn static_tokens(&self) -> Vec<String> {
        self.config.read().await.static_tokens.clone()
    }
    pub async fn validate_config(&self) -> CertResult<()> {
        let config = self.config.read().await;
        if config.static_tokens.is_empty() {
            info!(
                "WARNING: No static tokens configured. API authentication will fail. \
                / 警告：未配置静态令牌。API 认证将失败。"
            );
        } else {
            info!(count = config.static_tokens.len(), "已配置静态令牌");
        }
        Ok(())
    }
}
