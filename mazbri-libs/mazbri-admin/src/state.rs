use super::readiness::ReadinessResult;
use prometheus::Registry;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
pub type BoxedReadinessCheck =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ReadinessResult> + Send>> + Send + Sync>;
/// 可选的路由热加载状态提供者（网关设置；其它服务可空）。
pub type RoutesStatusProvider = Arc<dyn Fn() -> serde_json::Value + Send + Sync>;
#[derive(Clone)]
pub struct AdminState {
    pub registry: Arc<Registry>,
    pub service_name: String,
    pub ready: Arc<AtomicBool>,
    pub health_check: Arc<dyn Fn() -> bool + Send + Sync>,
    pub readiness_check: BoxedReadinessCheck,
    /// 网关注入：GET /routes/status
    pub routes_status: Arc<RwLock<Option<RoutesStatusProvider>>>,
}
impl AdminState {
    #[must_use]
    pub fn new(service_name: String, registry: Arc<Registry>) -> Self {
        Self {
            registry,
            service_name,
            ready: Arc::new(AtomicBool::new(false)),
            health_check: Arc::new(|| true),
            readiness_check: Arc::new(|| Box::pin(async { ReadinessResult::healthy(vec![]) })),
            routes_status: Arc::new(RwLock::new(None)),
        }
    }
    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::SeqCst);
    }
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }
    /// 注册路由热加载状态快照提供者（网关专用）。
    pub fn set_routes_status_provider<F>(&self, provider: F)
    where
        F: Fn() -> serde_json::Value + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.routes_status.write() {
            *guard = Some(Arc::new(provider));
        }
    }
}
