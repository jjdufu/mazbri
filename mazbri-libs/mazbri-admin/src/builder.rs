use super::config::AdminConfig;
use super::readiness::ReadinessResult;
use super::server::AdminServer;
use super::state::{AdminState, BoxedReadinessCheck};
use anyhow::Result;
use prometheus::Registry;
use std::future::Future;
use std::sync::Arc;
pub struct AdminServerBuilder {
    config: AdminConfig,
    service_name: String,
    registry: Option<Arc<Registry>>,
    health_check: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    readiness_check: Option<BoxedReadinessCheck>,
}
impl AdminServerBuilder {
    pub fn new(service_name: impl Into<String>, port: u16) -> Self {
        Self {
            config: AdminConfig {
                enabled: true,
                bind_addr: "0.0.0.0".to_string(),
                port,
                metrics_path: "/metrics".to_string(),
                health_path: "/health".to_string(),
                ready_path: "/ready".to_string(),
            },
            service_name: service_name.into(),
            registry: None,
            health_check: None,
            readiness_check: None,
        }
    }
    #[must_use]
    pub fn bind_addr(mut self, addr: impl Into<String>) -> Self {
        self.config.bind_addr = addr.into();
        self
    }
    #[must_use]
    pub fn registry(mut self, registry: Arc<Registry>) -> Self {
        self.registry = Some(registry);
        self
    }
    #[must_use]
    pub fn health_check<F>(mut self, check: F) -> Self
    where
        F: Fn() -> bool + Send + Sync + 'static,
    {
        self.health_check = Some(Arc::new(check));
        self
    }
    #[must_use]
    pub fn readiness_check<F, Fut>(mut self, check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ReadinessResult> + Send + 'static,
    {
        self.readiness_check = Some(Arc::new(move || Box::pin(check())));
        self
    }
    ///
    /// # Errors
    ///
    /// Returns an error if the configured bind address cannot be parsed.
    pub fn build(self) -> Result<AdminServer> {
        let addr = self.config.socket_addr()?;
        let registry = self.registry.unwrap_or_else(|| Arc::new(Registry::new()));
        let health_check = self.health_check.unwrap_or_else(|| Arc::new(|| true));
        let readiness_check = self
            .readiness_check
            .unwrap_or_else(|| Arc::new(|| Box::pin(async { ReadinessResult::healthy(vec![]) })));
        let state = AdminState {
            registry,
            service_name: self.service_name,
            ready: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            health_check,
            readiness_check,
            routes_status: Arc::new(std::sync::RwLock::new(None)),
        };
        Ok(AdminServer {
            addr,
            state,
            config: self.config,
        })
    }
}
