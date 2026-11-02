use super::{MetricsBuilder, register_collector_once};
use crate::REGISTRY;
use chrono::{DateTime, Utc};
use prometheus::{GaugeVec, IntGaugeVec, Opts};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::LazyLock;
use tokio::time::{Duration, interval};
pub trait DbPoolInfo: Send + Sync {
    fn pool_size(&self) -> u32;
    fn num_idle(&self) -> u32;
    fn is_healthy(&self) -> bool;
}
pub trait CertExpiryInfoProvider: Send + Sync {
    fn cert_expiry(&self) -> Option<DateTime<Utc>>;
    fn is_cert_valid(&self) -> bool;
}
pub type BoxedConfigCenterCheck =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;
pub static RPC_HEALTH_CHECK: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    MetricsBuilder::new().gauge_vec(
        "mazbri_rpc_health_check",
        "RPC service health check status (1=healthy, 0=unhealthy)",
        &["service", "status"],
    )
});
pub static DB_POOL_ACTIVE_CONNECTIONS: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    MetricsBuilder::new().gauge_vec(
        "mazbri_db_pool_active_connections",
        "Number of active database connections",
        &["service"],
    )
});
pub static DB_POOL_IDLE_CONNECTIONS: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    MetricsBuilder::new().gauge_vec(
        "mazbri_db_pool_idle_connections",
        "Number of idle database connections",
        &["service"],
    )
});
pub static TLS_CERT_EXPIRY_TIMESTAMP: LazyLock<GaugeVec> = LazyLock::new(|| {
    let name = "mazbri_tls_cert_expiry_timestamp";
    let help = "Unix timestamp of TLS certificate expiry";
    GaugeVec::new(Opts::new(name, help), &["service"])
        .expect("Failed to create TLS cert expiry gauge")
});
pub static CONFIG_CENTER_CONNECTED: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    MetricsBuilder::new().gauge_vec(
        "mazbri_config_center_connected",
        "Config center connection status (1=connected, 0=disconnected)",
        &["service"],
    )
});
/// 服务就绪（与 HTTP `/ready` 同源语义：1=可接流量，0=未就绪）。
pub static RPC_READY: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    MetricsBuilder::new().gauge_vec(
        "mazbri_rpc_ready",
        "Service readiness status aligned with /ready (1=ready, 0=not ready)",
        &["service"],
    )
});
pub type BoxedReadyCheck = Arc<dyn Fn() -> bool + Send + Sync>;

/// 立即写入 `mazbri_rpc_ready{service}`，供启动完成或 `/ready` 探针旁路同步。
pub fn set_rpc_ready(service: &str, ready: bool) {
    RPC_READY
        .with_label_values(&[service])
        .set(i64::from(ready));
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_health_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(RPC_HEALTH_CHECK.clone()))?;
    register_collector_once(&REGISTRY, Box::new(DB_POOL_ACTIVE_CONNECTIONS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(DB_POOL_IDLE_CONNECTIONS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(TLS_CERT_EXPIRY_TIMESTAMP.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CONFIG_CENTER_CONNECTED.clone()))?;
    register_collector_once(&REGISTRY, Box::new(RPC_READY.clone()))?;
    Ok(())
}
pub struct HealthMetricsUpdater {
    service_name: String,
    db_pool: Option<Arc<dyn DbPoolInfo>>,
    cert_handle: Option<Arc<dyn CertExpiryInfoProvider>>,
    config_center_connected: Option<BoxedConfigCenterCheck>,
    /// 若提供则作为就绪判定（应与 `/ready` 相同）；否则用依赖健康（db∧cert）作为就绪。
    ready_check: Option<BoxedReadyCheck>,
}
impl HealthMetricsUpdater {
    pub fn new(
        service_name: impl Into<String>,
        db_pool: Option<Arc<dyn DbPoolInfo>>,
        cert_handle: Option<Arc<dyn CertExpiryInfoProvider>>,
        config_center_connected: Option<BoxedConfigCenterCheck>,
    ) -> Self {
        Self {
            service_name: service_name.into(),
            db_pool,
            cert_handle,
            config_center_connected,
            ready_check: None,
        }
    }
    #[must_use]
    pub fn with_ready_check(mut self, check: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        self.ready_check = Some(Arc::new(check));
        self
    }
    pub async fn update(&self) {
        let name: &str = &self.service_name;
        let db_healthy = if let Some(ref pool) = self.db_pool {
            let active = pool.pool_size();
            let idle = pool.num_idle();
            DB_POOL_ACTIVE_CONNECTIONS
                .with_label_values(&[name])
                .set(i64::from(active));
            DB_POOL_IDLE_CONNECTIONS
                .with_label_values(&[name])
                .set(i64::from(idle));
            pool.is_healthy()
        } else {
            // 未接入数据库的服务不以连接池判定健康，与未配置证书一致。
            DB_POOL_ACTIVE_CONNECTIONS.with_label_values(&[name]).set(0);
            DB_POOL_IDLE_CONNECTIONS.with_label_values(&[name]).set(0);
            true
        };
        let cert_healthy = if let Some(ref cert) = self.cert_handle {
            if let Some(expires_at) = cert.cert_expiry() {
                TLS_CERT_EXPIRY_TIMESTAMP
                    .with_label_values(&[name])
                    .set(crate::num::i64_to_f64(expires_at.timestamp()));
            }
            cert.is_cert_valid()
        } else {
            true
        };
        let rpc_healthy = db_healthy && cert_healthy;
        RPC_HEALTH_CHECK
            .with_label_values(&[name, "healthy"])
            .set(i64::from(rpc_healthy));
        RPC_HEALTH_CHECK
            .with_label_values(&[name, "unhealthy"])
            .set(i64::from(!rpc_healthy));
        // 就绪与 /ready 同源：显式 ready_check（AdminState 标志）优先；否则依赖健康。
        let ready = self
            .ready_check
            .as_ref()
            .map_or(rpc_healthy, |check| check());
        set_rpc_ready(name, ready);
        if let Some(ref checker) = self.config_center_connected {
            let connected = checker().await;
            CONFIG_CENTER_CONNECTED
                .with_label_values(&[name])
                .set(i64::from(connected));
        }
        tracing::trace!(
            service = %name, db_healthy = db_healthy, cert_healthy = cert_healthy,
            rpc_healthy = rpc_healthy, ready = ready, "Health metrics updated"
        );
    }
    #[must_use]
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(15));
            loop {
                tokio::select! {
                    _ = interval.tick() => { self.update(). await; } _ =
                    tokio::signal::ctrl_c() => { tracing::info!(service = % self
                    .service_name, "Health metrics updater shutting down"); break; }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedPool(bool);
    impl DbPoolInfo for FixedPool {
        fn pool_size(&self) -> u32 {
            4
        }
        fn num_idle(&self) -> u32 {
            2
        }
        fn is_healthy(&self) -> bool {
            self.0
        }
    }

    struct FixedCert(bool);
    impl CertExpiryInfoProvider for FixedCert {
        fn cert_expiry(&self) -> Option<DateTime<Utc>> {
            None
        }
        fn is_cert_valid(&self) -> bool {
            self.0
        }
    }

    fn health(name: &str) -> (i64, i64) {
        (
            RPC_HEALTH_CHECK.with_label_values(&[name, "healthy"]).get(),
            RPC_HEALTH_CHECK
                .with_label_values(&[name, "unhealthy"])
                .get(),
        )
    }

    async fn run(
        name: &str,
        pool: Option<Arc<dyn DbPoolInfo>>,
        cert: Option<Arc<dyn CertExpiryInfoProvider>>,
    ) -> (i64, i64) {
        HealthMetricsUpdater::new(name, pool, cert, None)
            .update()
            .await;
        health(name)
    }

    #[tokio::test]
    async fn service_without_db_pool_is_healthy() {
        assert_eq!(run("health-test-no-pool", None, None).await, (1, 0));
        let pool = DB_POOL_ACTIVE_CONNECTIONS.with_label_values(&["health-test-no-pool"]);
        assert_eq!(pool.get(), 0);
    }

    #[tokio::test]
    async fn db_pool_health_drives_status() {
        let ok: Arc<dyn DbPoolInfo> = Arc::new(FixedPool(true));
        let bad: Arc<dyn DbPoolInfo> = Arc::new(FixedPool(false));
        assert_eq!(run("health-test-pool-ok", Some(ok), None).await, (1, 0));
        assert_eq!(run("health-test-pool-bad", Some(bad), None).await, (0, 1));
        let active = DB_POOL_ACTIVE_CONNECTIONS.with_label_values(&["health-test-pool-ok"]);
        assert_eq!(active.get(), 4);
    }

    #[tokio::test]
    async fn invalid_cert_is_unhealthy_without_db_pool() {
        let cert: Arc<dyn CertExpiryInfoProvider> = Arc::new(FixedCert(false));
        assert_eq!(run("health-test-bad-cert", None, Some(cert)).await, (0, 1));
    }

    #[tokio::test]
    async fn ready_defaults_to_dependency_health() {
        let ok: Arc<dyn DbPoolInfo> = Arc::new(FixedPool(true));
        let bad: Arc<dyn DbPoolInfo> = Arc::new(FixedPool(false));
        HealthMetricsUpdater::new("health-test-ready-ok", Some(ok), None, None)
            .update()
            .await;
        assert_eq!(
            RPC_READY.with_label_values(&["health-test-ready-ok"]).get(),
            1
        );
        HealthMetricsUpdater::new("health-test-ready-bad", Some(bad), None, None)
            .update()
            .await;
        assert_eq!(
            RPC_READY.with_label_values(&["health-test-ready-bad"]).get(),
            0
        );
    }

    #[tokio::test]
    async fn ready_check_overrides_dependency_health() {
        let bad: Arc<dyn DbPoolInfo> = Arc::new(FixedPool(false));
        HealthMetricsUpdater::new("health-test-ready-override", Some(bad), None, None)
            .with_ready_check(|| true)
            .update()
            .await;
        assert_eq!(
            RPC_READY
                .with_label_values(&["health-test-ready-override"])
                .get(),
            1
        );
    }
}
