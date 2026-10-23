use crate::registry::error::RegistryResult;
use crate::registry::types::{
    ConfigApplyResult, ConfigPush, ConfigPushResult, RegistryHeartbeatResponse, ServiceMetadata,
    ValidatedConfig,
};
use async_trait::async_trait;
use std::any::Any;
#[async_trait]
pub trait ServiceRegistry: Send + Sync {
    async fn register(&self, metadata: &ServiceMetadata) -> RegistryResult<()>;
    async fn heartbeat(
        &self,
        ip: &str,
        port: u16,
        config_version: i32,
    ) -> RegistryResult<RegistryHeartbeatResponse>;
    async fn on_config_push(&self, push: &ConfigPush) -> RegistryResult<ConfigPushResult>;
    async fn validate_config(&self, content: &str) -> RegistryResult<ValidatedConfig>;
    async fn apply_config(&self, config: &ValidatedConfig) -> RegistryResult<ConfigApplyResult>;
    async fn shutdown(&self) -> RegistryResult<()>;
    fn is_ready(&self) -> bool;
    fn metadata(&self) -> ServiceMetadata;
}
#[async_trait]
pub trait ConfigHandler: Send + Sync {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    fn parse_config(&self, content: &str) -> RegistryResult<Box<dyn Any + Send>>;
    async fn validate(&self, content: &str) -> RegistryResult<ValidatedConfig>;
    async fn apply(&self, config: &ValidatedConfig) -> RegistryResult<ConfigApplyResult>;
    async fn rollback(
        &self,
        previous_config: &ValidatedConfig,
    ) -> RegistryResult<ConfigApplyResult>;
}
