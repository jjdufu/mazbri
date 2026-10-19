use async_trait::async_trait;
use mazbri_registry::{
    ConfigApplyResult, ConfigHandler, RegistryError, RegistryResult, ValidatedConfig,
};
use std::any::Any;
use tracing::{info, warn};
pub struct GatewayConfigHandler {
    service_name: String,
}
impl GatewayConfigHandler {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
        }
    }
}
#[async_trait]
impl ConfigHandler for GatewayConfigHandler {
    fn parse_config(&self, content: &str) -> RegistryResult<Box<dyn Any + Send>> {
        let _config: serde_json::Value = serde_json::from_str(content)
            .map_err(|e| RegistryError::ConfigInvalid(format!("配置解析失败: {}", e)))?;
        Ok(Box::new(_config))
    }
    async fn validate(&self, content: &str) -> RegistryResult<ValidatedConfig> {
        info!(
            service = % self.service_name, content_length = content.len(),
            "收到配置变更通知（仅记录，不应用）"
        );
        Ok(ValidatedConfig {
            content: content.to_string(),
            version: 0,
            content_hash: String::new(),
        })
    }
    async fn apply(&self, config: &ValidatedConfig) -> RegistryResult<ConfigApplyResult> {
        info!(
            service = % self.service_name, version = config.version,
            "配置推送已接收（Gateway 不做热更新，需重启生效）"
        );
        Ok(ConfigApplyResult {
            success: true,
            version: config.version,
            message: Some("配置已记录，Gateway 需重启生效".to_string()),
        })
    }
    async fn rollback(
        &self,
        _previous_config: &ValidatedConfig,
    ) -> RegistryResult<ConfigApplyResult> {
        warn!(
            service = % self.service_name,
            "配置回滚请求已收到（Gateway 不做热更新）"
        );
        Ok(ConfigApplyResult {
            success: true,
            version: _previous_config.version,
            message: Some("Gateway 不支持热更新回滚".to_string()),
        })
    }
}
pub fn create_gateway_config_handler() -> std::sync::Arc<GatewayConfigHandler> {
    std::sync::Arc::new(GatewayConfigHandler::new("gateway"))
}
