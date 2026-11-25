use super::{MetricsBuilder, register_collector_once};
use crate::REGISTRY;
use crate::rate_limited_log::RateLimitedLog;
pub use async_trait::async_trait;
use chrono::{DateTime, Utc};
use prometheus::{GaugeVec, IntGaugeVec, Opts};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, LazyLock, OnceLock};
use tokio::time::{Duration, interval};
use tokio_util::sync::CancellationToken;

static NEAR_EXPIRY_WARN_GATE: LazyLock<RateLimitedLog> =
    LazyLock::new(|| RateLimitedLog::new(Duration::from_secs(300)));

#[async_trait]
pub trait DbPoolInfo: Send + Sync {
    fn pool_size(&self) -> u32;
    fn num_idle(&self) -> u32;
    async fn is_healthy(&self) -> bool;
}
pub type LateDbPool = Arc<OnceLock<Arc<dyn DbPoolInfo>>>;
pub trait CertExpiryInfoProvider: Send + Sync {
    fn cert_expiry(&self) -> Option<DateTime<Utc>>;
    fn is_cert_valid(&self) -> bool;
    fn is_cert_near_expiry(&self) -> bool {
        false
    }
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
/// 立即写入 `mazbri_db_pool_active_connections` 与 `mazbri_db_pool_idle_connections`。
///
/// 注意：`DbPoolInfo::pool_size()` 返回总连接数（活跃 + 空闲），
/// 因此真实活跃连接数必须按 `pool_size().saturating_sub(num_idle())` 计算。
pub fn record_db_pool_metrics(service: &str, pool: &dyn DbPoolInfo) {
    let idle = pool.num_idle();
    let active = pool.pool_size().saturating_sub(idle);
    DB_POOL_ACTIVE_CONNECTIONS
        .with_label_values(&[service])
        .set(i64::from(active));
    DB_POOL_IDLE_CONNECTIONS
        .with_label_values(&[service])
        .set(i64::from(idle));
}

pub struct HealthMetricsUpdater {
    service_name: String,
    db_pool: Option<Arc<dyn DbPoolInfo>>,
    late_db_pool: Option<LateDbPool>,
    cert_handle: Option<Arc<dyn CertExpiryInfoProvider>>,
    config_center_connected: Option<BoxedConfigCenterCheck>,
    /// 若提供则作为就绪判定（应与 `/ready` 相同）；否则用依赖健康（db∧cert）作为就绪。
    ready_check: Option<BoxedReadyCheck>,
    cancel_token: Option<CancellationToken>,
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
            late_db_pool: None,
            cert_handle,
            config_center_connected,
            ready_check: None,
            cancel_token: None,
        }
    }
    #[must_use]
    pub fn with_late_db_pool(mut self, slot: LateDbPool) -> Self {
        self.late_db_pool = Some(slot);
        self
    }
    #[must_use]
    pub fn with_ready_check(mut self, check: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        self.ready_check = Some(Arc::new(check));
        self
    }
    #[must_use]
    pub fn with_cancellation_token(mut self, token: CancellationToken) -> Self {
        self.cancel_token = Some(token);
        self
    }
    pub async fn update(&self) {
        let name: &str = &self.service_name;
        let effective_pool = self
            .db_pool
            .as_ref()
            .or_else(|| self.late_db_pool.as_ref().and_then(|slot| slot.get()));
        let db_healthy = if let Some(pool) = effective_pool {
            record_db_pool_metrics(name, pool.as_ref());
            pool.is_healthy().await
        } else {
            // 未接入数据库的服务不以连接池判定健康，与未配置证书一致。
            DB_POOL_ACTIVE_CONNECTIONS.with_label_values(&[name]).set(0);
            DB_POOL_IDLE_CONNECTIONS.with_label_values(&[name]).set(0);
            true
        };
        let cert_healthy = if let Some(ref cert) = self.cert_handle {
            let expiry = cert.cert_expiry();
            if let Some(expires_at) = expiry {
                TLS_CERT_EXPIRY_TIMESTAMP
                    .with_label_values(&[name])
                    .set(crate::num::i64_to_f64(expires_at.timestamp()));
            }
            let valid = cert.is_cert_valid();
            if valid && cert.is_cert_near_expiry() && NEAR_EXPIRY_WARN_GATE.allow(name) {
                tracing::warn!(
                    service = %name,
                    expires_at = ?expiry,
                    "TLS certificate is near expiry threshold; keeping service ready while awaiting renewal"
                );
            }
            valid
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
    pub fn spawn_with_token(self, token: CancellationToken) -> tokio::task::JoinHandle<()> {
        self.with_cancellation_token(token).spawn()
    }
    #[must_use]
    pub fn spawn(mut self) -> tokio::task::JoinHandle<()> {
        let cancel_token = self.cancel_token.take();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(15));
            if let Some(token) = cancel_token {
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            self.update().await;
                        }
                        () = token.cancelled() => {
                            tracing::info!(service = %self.service_name, "Health metrics updater shutting down (token)");
                            break;
                        }
                    }
                }
            } else {
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            self.update().await;
                        }
                        () = wait_for_termination_signal_internal() => {
                            tracing::info!(service = %self.service_name, "Health metrics updater shutting down");
                            break;
                        }
                    }
                }
            }
        })
    }
}

async fn wait_for_termination_signal_internal() {
    #[cfg(unix)]
    {
        if let Ok(mut sigterm) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = sigterm.recv() => {}
            }
        } else {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedPool(bool);
    #[async_trait]
    impl DbPoolInfo for FixedPool {
        fn pool_size(&self) -> u32 {
            4
        }
        fn num_idle(&self) -> u32 {
            2
        }
        async fn is_healthy(&self) -> bool {
            self.0
        }
    }

    struct FixedCert {
        valid: bool,
        near_expiry: bool,
    }
    impl CertExpiryInfoProvider for FixedCert {
        fn cert_expiry(&self) -> Option<DateTime<Utc>> {
            None
        }
        fn is_cert_valid(&self) -> bool {
            self.valid
        }
        fn is_cert_near_expiry(&self) -> bool {
            self.near_expiry
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
        let idle = DB_POOL_IDLE_CONNECTIONS.with_label_values(&["health-test-pool-ok"]);
        assert_eq!(active.get(), 2);
        assert_eq!(idle.get(), 2);
    }

    struct IdlePool;
    #[async_trait]
    impl DbPoolInfo for IdlePool {
        fn pool_size(&self) -> u32 {
            5
        }
        fn num_idle(&self) -> u32 {
            5
        }
        async fn is_healthy(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn idle_pool_reports_zero_active_connections() {
        let pool: Arc<dyn DbPoolInfo> = Arc::new(IdlePool);
        assert_eq!(run("health-test-idle-pool", Some(pool), None).await, (1, 0));
        assert_eq!(
            DB_POOL_ACTIVE_CONNECTIONS
                .with_label_values(&["health-test-idle-pool"])
                .get(),
            0
        );
        assert_eq!(
            DB_POOL_IDLE_CONNECTIONS
                .with_label_values(&["health-test-idle-pool"])
                .get(),
            5
        );
    }

    #[tokio::test]
    async fn late_db_pool_updates_metrics_and_health_after_set() {
        let slot: LateDbPool = Arc::new(OnceLock::new());
        let updater = HealthMetricsUpdater::new("health-test-late-pool", None, None, None)
            .with_late_db_pool(Arc::clone(&slot));

        updater.update().await;
        assert_eq!(health("health-test-late-pool"), (1, 0));
        assert_eq!(
            DB_POOL_ACTIVE_CONNECTIONS
                .with_label_values(&["health-test-late-pool"])
                .get(),
            0
        );
        assert_eq!(
            DB_POOL_IDLE_CONNECTIONS
                .with_label_values(&["health-test-late-pool"])
                .get(),
            0
        );

        let _ = slot.set(Arc::new(FixedPool(false)));
        updater.update().await;
        assert_eq!(health("health-test-late-pool"), (0, 1));
        assert_eq!(
            DB_POOL_ACTIVE_CONNECTIONS
                .with_label_values(&["health-test-late-pool"])
                .get(),
            2
        );
        assert_eq!(
            DB_POOL_IDLE_CONNECTIONS
                .with_label_values(&["health-test-late-pool"])
                .get(),
            2
        );
    }

    #[tokio::test]
    async fn invalid_cert_is_unhealthy_without_db_pool() {
        let cert: Arc<dyn CertExpiryInfoProvider> = Arc::new(FixedCert {
            valid: false,
            near_expiry: true,
        });
        assert_eq!(run("health-test-bad-cert", None, Some(cert)).await, (0, 1));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn near_expiry_unexpired_cert_keeps_rpc_ready_one_on_current_thread() {
        let near_cert: Arc<dyn CertExpiryInfoProvider> = Arc::new(FixedCert {
            valid: true,
            near_expiry: true,
        });
        HealthMetricsUpdater::new("health-test-near-cert", None, Some(near_cert), None)
            .update()
            .await;
        assert_eq!(health("health-test-near-cert"), (1, 0));
        assert_eq!(
            RPC_READY
                .with_label_values(&["health-test-near-cert"])
                .get(),
            1
        );

        let expired_cert: Arc<dyn CertExpiryInfoProvider> = Arc::new(FixedCert {
            valid: false,
            near_expiry: true,
        });
        HealthMetricsUpdater::new("health-test-expired-cert", None, Some(expired_cert), None)
            .update()
            .await;
        assert_eq!(health("health-test-expired-cert"), (0, 1));
        assert_eq!(
            RPC_READY
                .with_label_values(&["health-test-expired-cert"])
                .get(),
            0
        );
    }

    #[tokio::test]
    async fn cancellation_token_stops_updater_task_cleanly() {
        let token = CancellationToken::new();
        let handle = HealthMetricsUpdater::new("health-test-cancel-token", None, None, None)
            .spawn_with_token(token.clone());
        tokio::task::yield_now().await;
        token.cancel();
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("updater task must exit on token cancellation")
            .expect("updater task must not panic");
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
