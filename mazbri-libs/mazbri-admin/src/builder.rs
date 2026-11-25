use super::config::AdminConfig;
use super::readiness::{CheckStatus, ReadinessResult};
use super::server::AdminServer;
use super::state::{AdminState, BoxedReadinessCheck};
use anyhow::Result;
use prometheus::Registry;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;

type StatefulReadinessCheck = Arc<
    dyn Fn(Arc<AdminState>) -> Pin<Box<dyn Future<Output = ReadinessResult> + Send>> + Send + Sync,
>;

enum ReadinessSource {
    Stateless(BoxedReadinessCheck),
    WithState(StatefulReadinessCheck),
}

pub struct AdminServerBuilder {
    config: AdminConfig,
    listen_override: Option<(IpAddr, u16)>,
    service_name: String,
    registry: Option<Arc<Registry>>,
    health_check: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    readiness_source: Option<ReadinessSource>,
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
            listen_override: None,
            service_name: service_name.into(),
            registry: None,
            health_check: None,
            readiness_source: None,
        }
    }
    #[must_use]
    pub fn bind_addr(mut self, addr: impl Into<String>) -> Self {
        self.config.bind_addr = addr.into();
        self.listen_override = None;
        self
    }
    #[must_use]
    pub fn listen_addr(mut self, addr: (IpAddr, u16)) -> Self {
        self.config.bind_addr = addr.0.to_string();
        self.config.port = addr.1;
        self.listen_override = Some(addr);
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
        self.readiness_source = Some(ReadinessSource::Stateless(Arc::new(move || {
            Box::pin(check())
        })));
        self
    }
    #[must_use]
    pub fn readiness_check_with_state<F, Fut>(mut self, check: F) -> Self
    where
        F: Fn(Arc<AdminState>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ReadinessResult> + Send + 'static,
    {
        self.readiness_source = Some(ReadinessSource::WithState(Arc::new(move |state| {
            Box::pin(check(state))
        })));
        self
    }
    ///
    /// # Errors
    ///
    /// Returns an error if the configured bind address cannot be parsed.
    pub fn build(self) -> Result<AdminServer> {
        let addr = match self.listen_override {
            Some(pair) => SocketAddr::from(pair),
            None => self.config.socket_addr()?,
        };
        let registry = self.registry.unwrap_or_else(|| Arc::new(Registry::new()));
        let health_check = self.health_check.unwrap_or_else(|| Arc::new(|| true));
        let readiness_source = self.readiness_source;
        let service_name = self.service_name;
        let state = Arc::new_cyclic(|weak_state| {
            let readiness_check: BoxedReadinessCheck = match readiness_source {
                Some(ReadinessSource::Stateless(check)) => check,
                Some(ReadinessSource::WithState(check)) => {
                    let weak = weak_state.clone();
                    Arc::new(move || {
                        if let Some(st) = weak.upgrade() {
                            check(st)
                        } else {
                            Box::pin(async {
                                ReadinessResult {
                                    status: CheckStatus::Failed,
                                    checks: vec![],
                                }
                            })
                        }
                    })
                }
                None => Arc::new(|| Box::pin(async { ReadinessResult::healthy(vec![]) })),
            };
            AdminState {
                registry,
                service_name,
                ready: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                health_check,
                readiness_check,
                routes_status: Arc::new(std::sync::RwLock::new(None)),
            }
        });
        Ok(AdminServer {
            addr,
            state,
            config: self.config,
        })
    }
}
