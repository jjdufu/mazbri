use anyhow::Result;
use mazbri_admin::readiness::{CheckStatus, ReadinessResult};
use mazbri_admin::{AdminConfig, AdminServerBuilder, AdminState};
use mazbri_core::REGISTRY;
use mazbri_core::metrics::{HealthMetricsUpdater, register_health_metrics, set_rpc_ready};
use std::sync::Arc;
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn start_simple_admin_server(
    service_name: &str,
    admin_config: &AdminConfig,
) -> Result<Option<Arc<AdminState>>> {
    if !admin_config.enabled {
        tracing::info!("Admin server disabled");
        return Ok(None);
    }
    let state = Arc::new(AdminState::new(
        service_name.to_string(),
        Arc::new(REGISTRY.clone()),
    ));
    let state_for_check = state.clone();
    let server = AdminServerBuilder::new(service_name, admin_config.port)
        .bind_addr(&admin_config.bind_addr)
        .registry(Arc::new(REGISTRY.clone()))
        .readiness_check(move || {
            let ready = state_for_check.is_ready();
            async move {
                if ready {
                    ReadinessResult::healthy(vec![])
                } else {
                    ReadinessResult {
                        status: CheckStatus::Failed,
                        checks: vec![],
                    }
                }
            }
        })
        .build()?;
    let _handle = server.spawn();
    tracing::info!("Admin server started on port {}", admin_config.port);
    if let Err(e) = register_health_metrics() {
        tracing::debug!(
            "Health metrics already registered or registration failed: {}",
            e
        );
    }
    // 无 DB / 证书 / 配置中心句柄时传 None；就绪与 AdminState（=/ready）同源。
    let state_for_ready = state.clone();
    let updater = HealthMetricsUpdater::new(service_name.to_string(), None, None, None)
        .with_ready_check(move || state_for_ready.is_ready());
    drop(updater.spawn());
    // 启动瞬间先写 0，避免 scrape 读到缺失序列；真正置位由 set_ready/set_rpc_ready。
    set_rpc_ready(service_name, state.is_ready());
    tracing::info!("Health metrics updater started for {}", service_name);
    Ok(Some(state))
}
