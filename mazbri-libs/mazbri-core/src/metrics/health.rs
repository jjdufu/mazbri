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
pub fn register_health_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(RPC_HEALTH_CHECK.clone()))?;
    register_collector_once(&REGISTRY, Box::new(DB_POOL_ACTIVE_CONNECTIONS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(DB_POOL_IDLE_CONNECTIONS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(TLS_CERT_EXPIRY_TIMESTAMP.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CONFIG_CENTER_CONNECTED.clone()))?;
    Ok(())
}
pub struct HealthMetricsUpdater {
    service_name: String,
    db_pool: Option<Arc<dyn DbPoolInfo>>,
    cert_handle: Option<Arc<dyn CertExpiryInfoProvider>>,
    config_center_connected: Option<BoxedConfigCenterCheck>,
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
        }
    }
    pub async fn update(&self) {
        let name: &str = &self.service_name;
        let db_healthy = if let Some(ref pool) = self.db_pool {
            let active = pool.pool_size();
            let idle = pool.num_idle();
            DB_POOL_ACTIVE_CONNECTIONS
                .with_label_values(&[name])
                .set(active as i64);
            DB_POOL_IDLE_CONNECTIONS
                .with_label_values(&[name])
                .set(idle as i64);
            pool.is_healthy()
        } else {
            DB_POOL_ACTIVE_CONNECTIONS.with_label_values(&[name]).set(0);
            DB_POOL_IDLE_CONNECTIONS.with_label_values(&[name]).set(0);
            false
        };
        let cert_healthy = if let Some(ref cert) = self.cert_handle {
            if let Some(expires_at) = cert.cert_expiry() {
                TLS_CERT_EXPIRY_TIMESTAMP
                    .with_label_values(&[name])
                    .set(expires_at.timestamp() as f64);
            }
            cert.is_cert_valid()
        } else {
            true
        };
        let rpc_healthy = db_healthy && cert_healthy;
        RPC_HEALTH_CHECK
            .with_label_values(&[name, "healthy"])
            .set(if rpc_healthy { 1 } else { 0 });
        RPC_HEALTH_CHECK
            .with_label_values(&[name, "unhealthy"])
            .set(if rpc_healthy { 0 } else { 1 });
        if let Some(ref checker) = self.config_center_connected {
            let connected = checker().await;
            CONFIG_CENTER_CONNECTED
                .with_label_values(&[name])
                .set(if connected { 1 } else { 0 });
        }
        tracing::trace!(
            service = % name, db_healthy = db_healthy, cert_healthy = cert_healthy,
            rpc_healthy = rpc_healthy, "Health metrics updated"
        );
    }
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
