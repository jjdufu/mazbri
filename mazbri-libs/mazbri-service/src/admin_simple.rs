use anyhow::Result;
use mazbri_admin::readiness::{CheckStatus, ReadinessResult};
use mazbri_admin::{AdminConfig, AdminServerBuilder, AdminState};
use mazbri_core::REGISTRY;
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
    Ok(Some(state))
}
