use crate::registry::error::{RegistryError, RegistryResult};
use crate::registry::traits::ConfigHandler;
use crate::registry::types::{ConfigApplyResult, ValidatedConfig};
use async_trait::async_trait;
use mazbri_core::config::ValidateConfig;
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use std::any::Any;
use std::sync::Arc;
pub struct TomlConfigHandler<T> {
    current_config: Arc<RwLock<Option<T>>>,
    previous_config: Arc<RwLock<Option<T>>>,
    current_version: Arc<std::sync::atomic::AtomicI32>,
    current_content_hash: Arc<RwLock<String>>,
    _service_name: String,
}
impl<T> TomlConfigHandler<T> {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            current_config: Arc::new(RwLock::new(None)),
            previous_config: Arc::new(RwLock::new(None)),
            current_version: Arc::new(std::sync::atomic::AtomicI32::new(0)),
            current_content_hash: Arc::new(RwLock::new(String::new())),
            _service_name: service_name.into(),
        }
    }
    pub fn get_config(&self) -> RegistryResult<T>
    where
        T: Clone,
    {
        let guard = self.current_config.read();
        guard
            .clone()
            .ok_or_else(|| RegistryError::ConfigApplyFailed("当前无可用配置".to_string()))
    }
}
fn compute_hash(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}
#[async_trait]
impl<T: DeserializeOwned + Clone + ValidateConfig + Send + Sync + 'static> ConfigHandler
    for TomlConfigHandler<T>
{
    fn parse_config(&self, content: &str) -> RegistryResult<Box<dyn Any + Send>> {
        let config: T = toml::from_str(content)
            .map_err(|e| RegistryError::ConfigInvalid(format!("TOML 解析失败: {e}")))?;
        Ok(Box::new(config))
    }
    async fn validate(&self, content: &str) -> RegistryResult<ValidatedConfig> {
        let config: T = toml::from_str(content)
            .map_err(|e| RegistryError::ConfigInvalid(format!("TOML 解析失败: {e}")))?;
        config
            .validate()
            .map_err(|e| RegistryError::ConfigValidationFailed(format!("配置验证失败: {e}")))?;
        Ok(ValidatedConfig {
            content: content.to_string(),
            version: 0,
            content_hash: compute_hash(content),
        })
    }
    async fn apply(&self, config: &ValidatedConfig) -> RegistryResult<ConfigApplyResult> {
        let new_config: T = toml::from_str(&config.content)
            .map_err(|e| RegistryError::ConfigApplyFailed(format!("TOML 解析失败: {e}")))?;
        let mut current_guard = self.current_config.write();
        let mut previous_guard = self.previous_config.write();
        *previous_guard = current_guard.take();
        *current_guard = Some(new_config);
        self.current_version
            .store(config.version, std::sync::atomic::Ordering::Relaxed);
        let mut hash_guard = self.current_content_hash.write();
        *hash_guard = config.content_hash.clone();
        Ok(ConfigApplyResult {
            success: true,
            version: config.version,
            message: None,
        })
    }
    async fn rollback(
        &self,
        previous_config: &ValidatedConfig,
    ) -> RegistryResult<ConfigApplyResult> {
        let old_config: T = toml::from_str(&previous_config.content)
            .map_err(|e| RegistryError::ConfigApplyFailed(format!("回滚 TOML 解析失败: {e}")))?;
        let mut current_guard = self.current_config.write();
        let mut previous_guard = self.previous_config.write();
        if previous_guard.is_none() {
            return Err(RegistryError::ConfigApplyFailed(
                "无可用历史配置，无法回滚".to_string(),
            ));
        }
        *current_guard = Some(old_config);
        *previous_guard = None;
        self.current_version.store(
            previous_config.version,
            std::sync::atomic::Ordering::Relaxed,
        );
        let mut hash_guard = self.current_content_hash.write();
        *hash_guard = previous_config.content_hash.clone();
        Ok(ConfigApplyResult {
            success: true,
            version: previous_config.version,
            message: Some("rolled back".to_string()),
        })
    }
}
