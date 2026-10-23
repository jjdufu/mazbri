use crate::registry::error::RegistryResult;
use crate::registry::traits::ConfigHandler;
use crate::registry::types::{ConfigPush, ConfigPushResult, ValidatedConfig};
use std::sync::Arc;
pub struct ConfigPipeline {
    config_handler: Arc<dyn ConfigHandler>,
    previous_config: Arc<tokio::sync::RwLock<Option<ValidatedConfig>>>,
}
impl ConfigPipeline {
    pub fn new(config_handler: Arc<dyn ConfigHandler>) -> Self {
        Self {
            config_handler,
            previous_config: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }
    pub fn with_previous_config(
        config_handler: Arc<dyn ConfigHandler>,
        previous: ValidatedConfig,
    ) -> Self {
        Self {
            config_handler,
            previous_config: Arc::new(tokio::sync::RwLock::new(Some(previous))),
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn process_config_change(
        &self,
        push: &ConfigPush,
    ) -> RegistryResult<ConfigPushResult> {
        let validated_config = match self.config_handler.validate(&push.content).await {
            Ok(config) => config,
            Err(e) => {
                return Ok(ConfigPushResult {
                    accepted: false,
                    applied: false,
                    version: push.version,
                    message: Some(format!("validate failed: {e}")),
                });
            }
        };
        let old_config = self.previous_config.read().await.clone();
        match self.config_handler.apply(&validated_config).await {
            Ok(_) => {
                let mut prev = self.previous_config.write().await;
                *prev = Some(validated_config);
                drop(prev);
                Ok(ConfigPushResult {
                    accepted: true,
                    applied: true,
                    version: push.version,
                    message: None,
                })
            }
            Err(apply_err) => {
                if let Some(ref old) = old_config {
                    if self.config_handler.rollback(old).await.is_ok() {
                        Ok(ConfigPushResult {
                            accepted: true,
                            applied: false,
                            version: push.version,
                            message: Some(
                                "config apply failed, rolled back to previous version".to_string(),
                            ),
                        })
                    } else {
                        tracing::error!(
                            error = % apply_err, "config apply and rollback both failed"
                        );
                        Ok(ConfigPushResult {
                            accepted: true,
                            applied: false,
                            version: push.version,
                            message: Some("apply and rollback both failed".to_string()),
                        })
                    }
                } else {
                    Ok(ConfigPushResult {
                        accepted: true,
                        applied: false,
                        version: push.version,
                        message: Some(format!("config apply failed: {apply_err}")),
                    })
                }
            }
        }
    }
    pub async fn get_previous_config(&self) -> Option<ValidatedConfig> {
        self.previous_config.read().await.clone()
    }
}
