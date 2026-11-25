use crate::bootstrap::PgPoolInfoBridge;
use anyhow::Result;
use mazbri_admin::readiness::{CheckStatus, ReadinessCheckResult, ReadinessResult};
use mazbri_admin::{AdminConfig, AdminServerBuilder, AdminState};
use mazbri_core::REGISTRY;
use mazbri_core::metrics::{
    DbPoolInfo, HealthMetricsUpdater, LateDbPool, record_db_pool_metrics, register_health_metrics,
    set_rpc_ready,
};
use sqlx::PgPool;
use std::sync::{Arc, OnceLock};

/// 管理端口先行启动句柄：支持在 `create_pool` 完成后延迟绑定主库连接池，
/// 使 `/ready` 探针与 `mazbri_db_pool_*` 指标统一感知数据库状态。
#[derive(Clone)]
pub struct SimpleAdminHandle {
    state: Arc<AdminState>,
    service_name: String,
    db_pool: Arc<OnceLock<Arc<PgPool>>>,
    cert_handle: Arc<OnceLock<mazbri_tls::ManagedCertHandle>>,
    metrics_pool: LateDbPool,
}

impl SimpleAdminHandle {
    #[must_use]
    pub fn state(&self) -> &Arc<AdminState> {
        &self.state
    }

    pub fn set_ready(&self, ready: bool) {
        self.state.set_ready(ready);
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.state.is_ready()
    }

    /// 注入延迟创建的 PostgreSQL 主连接池并立即同步一次指标快照。
    pub fn set_db_pool(&self, pool: Arc<PgPool>) {
        let _ = self.db_pool.set(Arc::clone(&pool));
        let bridge: Arc<dyn DbPoolInfo> = Arc::new(PgPoolInfoBridge(pool));
        record_db_pool_metrics(&self.service_name, bridge.as_ref());
        let _ = self.metrics_pool.set(bridge);
    }

    /// 注入延迟初始化的动态 mTLS 证书句柄：仅在真实过期（`is_expired_now`）时将 `/ready` 置为 `503`。
    pub fn set_cert_handle(&self, handle: mazbri_tls::ManagedCertHandle) {
        if let Some(exp) = handle.cert_expiry() {
            mazbri_core::metrics::TLS_CERT_EXPIRY_TIMESTAMP
                .with_label_values(&[&self.service_name])
                .set(mazbri_core::num::i64_to_f64(exp.timestamp()));
        }
        let _ = self.cert_handle.set(handle);
    }
}

pub(crate) async fn evaluate_simple_readiness(
    ready: bool,
    db_pool: Option<&Arc<PgPool>>,
    cert_handle: Option<&mazbri_tls::ManagedCertHandle>,
) -> ReadinessResult {
    if !ready {
        return ReadinessResult {
            status: CheckStatus::Failed,
            checks: vec![],
        };
    }
    let mut checks = Vec::new();
    let mut failed = false;
    if let Some(cert) = cert_handle {
        if cert.is_expired_now() {
            failed = true;
            checks.push(ReadinessCheckResult {
                name: "certificate".to_string(),
                status: CheckStatus::Failed,
                message: Some("mTLS certificate is expired".to_string()),
            });
        } else {
            checks.push(ReadinessCheckResult {
                name: "certificate".to_string(),
                status: CheckStatus::Ok,
                message: None,
            });
        }
    }
    if let Some(pool) = db_pool {
        match tokio::time::timeout(std::time::Duration::from_secs(2), pool.acquire()).await {
            Ok(Ok(conn)) => {
                drop(conn);
                checks.push(ReadinessCheckResult {
                    name: "database".to_string(),
                    status: CheckStatus::Ok,
                    message: None,
                });
            }
            Ok(Err(e)) => {
                failed = true;
                checks.push(ReadinessCheckResult {
                    name: "database".to_string(),
                    status: CheckStatus::Failed,
                    message: Some(format!("database connection failed: {e}")),
                });
            }
            Err(_) => {
                failed = true;
                checks.push(ReadinessCheckResult {
                    name: "database".to_string(),
                    status: CheckStatus::Failed,
                    message: Some("database connection acquire timed out after 2s".to_string()),
                });
            }
        }
    }
    if failed {
        ReadinessResult {
            status: CheckStatus::Failed,
            checks,
        }
    } else {
        ReadinessResult::healthy(checks)
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn start_simple_admin_server(
    service_name: &str,
    admin_config: &AdminConfig,
) -> Result<Option<SimpleAdminHandle>> {
    if !admin_config.enabled {
        tracing::info!("Admin server disabled");
        return Ok(None);
    }
    let db_pool: Arc<OnceLock<Arc<PgPool>>> = Arc::new(OnceLock::new());
    let cert_handle: Arc<OnceLock<mazbri_tls::ManagedCertHandle>> = Arc::new(OnceLock::new());
    let metrics_pool: LateDbPool = Arc::new(OnceLock::new());

    let db_pool_for_check = Arc::clone(&db_pool);
    let cert_for_check = Arc::clone(&cert_handle);
    let server = AdminServerBuilder::new(service_name, admin_config.port)
        .bind_addr(&admin_config.bind_addr)
        .registry(Arc::new(REGISTRY.clone()))
        .readiness_check_with_state(move |state| {
            let ready = state.is_ready();
            let pool = db_pool_for_check.get().cloned();
            let cert = cert_for_check.get().cloned();
            async move { evaluate_simple_readiness(ready, pool.as_ref(), cert.as_ref()).await }
        })
        .build()?;
    let state = server.state();
    let _handle = server.spawn();
    tracing::info!("Admin server started on port {}", admin_config.port);
    if let Err(e) = register_health_metrics() {
        tracing::debug!(
            "Health metrics already registered or registration failed: {}",
            e
        );
    }
    let state_for_ready = state.clone();
    let cert_for_ready = Arc::clone(&cert_handle);
    let svc_for_ready = service_name.to_string();
    let updater = HealthMetricsUpdater::new(service_name.to_string(), None, None, None)
        .with_late_db_pool(Arc::clone(&metrics_pool))
        .with_ready_check(move || {
            if let Some(cert) = cert_for_ready.get() {
                if let Some(exp) = cert.cert_expiry() {
                    mazbri_core::metrics::TLS_CERT_EXPIRY_TIMESTAMP
                        .with_label_values(&[&svc_for_ready])
                        .set(mazbri_core::num::i64_to_f64(exp.timestamp()));
                }
                if cert.is_expired_now() {
                    return false;
                }
            }
            state_for_ready.is_ready()
        });
    drop(updater.spawn());
    // 启动瞬间先写 0，避免 scrape 读到缺失序列；真正置位由 set_ready/set_rpc_ready。
    set_rpc_ready(service_name, state.is_ready());
    tracing::info!("Health metrics updater started for {}", service_name);
    Ok(Some(SimpleAdminHandle {
        state,
        service_name: service_name.to_string(),
        db_pool,
        cert_handle,
        metrics_pool,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn readiness_respects_ready_flag_and_late_pool() {
        let not_ready = evaluate_simple_readiness(false, None, None).await;
        assert_eq!(not_ready.status, CheckStatus::Failed);

        let ready_no_pool = evaluate_simple_readiness(true, None, None).await;
        assert_eq!(ready_no_pool.status, CheckStatus::Ok);
        assert!(ready_no_pool.checks.is_empty());
    }
}
