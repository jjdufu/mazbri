use crate::http_config_client::{HttpConfigClient, HttpConfigClientError};
use crate::registry::connection::ConnectionManager;
use crate::registry::error::{RegistryError, RegistryResult};
use crate::registry::pipeline::ConfigPipeline;
use crate::registry::traits::{ConfigHandler, ServiceRegistry};
use crate::registry::types::{
    ConfigApplyResult, ConfigPush, ConfigPushResult, RegistryHeartbeatResponse, ServiceMetadata,
    ValidatedConfig,
};
use async_trait::async_trait;
use mazbri_core::CoreError;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
pub struct ConfigCenterRegistry {
    http_client: Arc<HttpConfigClient>,
    config_handler: Arc<dyn ConfigHandler>,
    pipeline: Arc<ConfigPipeline>,
    connection_manager: Arc<ConnectionManager>,
    metadata: Arc<parking_lot::RwLock<ServiceMetadata>>,
    ready: AtomicBool,
    shutdown_token: CancellationToken,
    heartbeat_handle: tokio::sync::RwLock<Option<tokio::task::JoinHandle<()>>>,
    pipeline_mutex: Arc<Mutex<()>>,
}
impl ConfigCenterRegistry {
    pub fn new(
        http_client: Arc<HttpConfigClient>,
        config_handler: Arc<dyn ConfigHandler>,
        metadata: ServiceMetadata,
    ) -> Self {
        let pipeline = ConfigPipeline::new(config_handler.clone());
        Self {
            http_client,
            config_handler,
            pipeline: Arc::new(pipeline),
            connection_manager: Arc::new(ConnectionManager::new()),
            metadata: Arc::new(parking_lot::RwLock::new(metadata)),
            ready: AtomicBool::new(false),
            shutdown_token: CancellationToken::new(),
            heartbeat_handle: tokio::sync::RwLock::new(None),
            pipeline_mutex: Arc::new(Mutex::new(())),
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn start(&self) -> RegistryResult<()> {
        let handle = super::heartbeat::spawn_heartbeat_task(
            self.shutdown_token.clone(),
            self.http_client.clone(),
            self.metadata.clone(),
            self.pipeline.clone(),
            self.pipeline_mutex.clone(),
        )
        .await;
        let mut handle_lock = self.heartbeat_handle.write().await;
        *handle_lock = Some(handle);
        self.ready.store(true, Ordering::Relaxed);
        Ok(())
    }
    pub fn connection_manager(&self) -> &Arc<ConnectionManager> {
        &self.connection_manager
    }
    #[must_use]
    pub fn map_http_error(e: HttpConfigClientError) -> RegistryError {
        match e {
            HttpConfigClientError::Core(core) => RegistryError::Core(core),
            HttpConfigClientError::RequestFailed(msg) => {
                RegistryError::Core(CoreError::Internal(msg))
            }
            HttpConfigClientError::RegistrationFailed(msg) => RegistryError::ConfigApplyFailed(msg),
            other => RegistryError::ConfigApplyFailed(other.to_string()),
        }
    }
}
#[async_trait]
impl ServiceRegistry for ConfigCenterRegistry {
    async fn register(&self, metadata: &ServiceMetadata) -> RegistryResult<()> {
        self.http_client
            .register_node(
                metadata.ip.clone(),
                metadata.port,
                metadata.health_check_url.clone(),
                metadata.started_at,
            )
            .await
            .map_err(Self::map_http_error)?;
        {
            let mut meta = self.metadata.write();
            *meta = metadata.clone();
        }
        info!(
            service = % metadata.service_name, ip = % metadata.ip, port = metadata.port,
            "节点注册成功"
        );
        Ok(())
    }
    async fn heartbeat(
        &self,
        ip: &str,
        port: u16,
        _config_version: i32,
    ) -> RegistryResult<RegistryHeartbeatResponse> {
        let resp = self
            .http_client
            .send_heartbeat(ip, port)
            .await
            .map_err(Self::map_http_error)?;
        Ok(RegistryHeartbeatResponse {
            success: resp.success,
            error_message: resp.error_message,
            config_changed: resp.config_changed,
            latest_version: resp.latest_version,
        })
    }
    async fn on_config_push(&self, push: &ConfigPush) -> RegistryResult<ConfigPushResult> {
        let _guard = self.pipeline_mutex.lock().await;
        self.pipeline.process_config_change(push).await
    }
    async fn validate_config(&self, content: &str) -> RegistryResult<ValidatedConfig> {
        self.config_handler.validate(content).await
    }
    async fn apply_config(&self, config: &ValidatedConfig) -> RegistryResult<ConfigApplyResult> {
        self.config_handler.apply(config).await
    }
    async fn shutdown(&self) -> RegistryResult<()> {
        info!("开始关闭注册中心");
        self.shutdown_token.cancel();
        let timeout = Duration::from_secs(5);
        {
            let mut handle = self.heartbeat_handle.write().await;
            if let Some(h) = handle.take() {
                if let Ok(_) = tokio::time::timeout(timeout, h).await {
                    info!("心跳任务已停止");
                } else {
                    warn!("心跳任务超时未停止");
                }
            }
        }
        self.ready.store(false, Ordering::Relaxed);
        info!("注册中心已关闭");
        Ok(())
    }
    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }
    fn metadata(&self) -> ServiceMetadata {
        self.metadata.read().clone()
    }
}
