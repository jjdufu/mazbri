use anyhow::Result;
use chrono::Utc;
use mazbri_admin::readiness::{CheckStatus, ReadinessResult};
use mazbri_admin::{AdminServerBuilder, AdminState};
use mazbri_cert_service::auth::AuthState;
use mazbri_cert_service::base_authorization;
use mazbri_cert_service::config::CertServiceConfig;
use mazbri_cert_service::config_center::ConfigRepository;
use mazbri_cert_service::db::CertificateRepository;
use mazbri_cert_service::handlers::certificate::common::update_certificate_inventory_metrics;
use mazbri_cert_service::node_registry::NodeRegistryRepository;
use mazbri_cert_service::server;
use mazbri_cert_service::service_registry::ServiceRepository;
use mazbri_cert_service::token::TokenManager;
use mazbri_core::config_loader::{BootstrapConfig, ValidateConfig, get_cert_service_token};
use mazbri_core::crypto::tenant_key::RootProtectionMaterialBundle;
use mazbri_core::init_tracing_with_routes;
use parking_lot::Mutex;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

async fn sync_authorization_expiry(
    repo: &Arc<CertificateRepository>,
    _base_authorization: &Arc<base_authorization::BaseAuthorizationState>,
    root_material_cache: Option<&Arc<Mutex<Option<RootProtectionMaterialBundle>>>>,
) {
    let pool = repo.get_pool();
    if let Err(error) = mazbri_cert_service::license_service::sync_runtime(pool, Utc::now()).await {
        tracing::warn!("Failed to sync commercial license runtime: {}", error);
    }
    let _ = root_material_cache;
}

#[tokio::main]
async fn main() -> Result<()> {
    let config_dir = Path::new(".");
    let bootstrap_path = mazbri_core::config_loader::resolve_bootstrap_path_mandatory()?;
    let mut config = CertServiceConfig::load_from_file(&bootstrap_path)?;
    let bootstrap = BootstrapConfig::load_from_path(&bootstrap_path)?;
    let service_name = config.service_name_or("mazbri-cert-service");
    let log_routes = bootstrap.log_routes()?;
    let cert_service_token = get_cert_service_token()?;
    config.auth.static_tokens = vec![cert_service_token];
    config.cert.key_password = None;
    config.validate()?;
    let admin_state: Option<Arc<AdminState>> = if config.admin.enabled {
        let state = Arc::new(AdminState::new(
            service_name.clone(),
            Arc::new(mazbri_core::REGISTRY.clone()),
        ));
        let state_for_check = state.clone();
        let server = AdminServerBuilder::new(&service_name, config.admin.port)
            .registry(Arc::new(mazbri_core::REGISTRY.clone()))
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
        info!("Admin server started on port {}", config.admin.port);
        Some(state)
    } else {
        info!("Admin server disabled");
        None
    };
    let _log_guard = init_tracing_with_routes(&bootstrap.tracing, &service_name, &log_routes)?;
    mazbri_tls::register_cert_metrics()?;
    info!("Mazbri 证书服务启动中...");
    info!(host = % config.server.host, port = % config.server.port, "服务器配置");
    let db = mazbri_core::create_pool(&config.database, Some(config_dir)).await?;
    info!("已连接数据库");
    let repo = Arc::new(CertificateRepository::new(db.clone()));
    update_certificate_inventory_metrics(&repo).await;
    {
        let repo = repo.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                update_certificate_inventory_metrics(&repo).await;
            }
        });
    }
    if let Some(ref state) = admin_state {
        state.set_ready(true);
        info!("Service marked as ready for authorization import");
    }
    let token_manager = Arc::new(TokenManager::new(Arc::new(config.auth.clone())));
    token_manager.validate_config().await?;
    let auth_state = Arc::new(AuthState::new(token_manager.clone(), &config.auth));
    let base_authorization = Arc::new(base_authorization::BaseAuthorizationState::new(
        repo.clone(),
        auth_state.clone(),
    ));
    let node_repo = Arc::new(NodeRegistryRepository::new(db.clone()));
    info!("节点注册仓库已初始化");
    let config_repo = Arc::new(ConfigRepository::new(db.clone()));
    info!("配置仓库已初始化");
    let service_registry_repo = Arc::new(ServiceRepository::new(db.clone()));
    info!("服务注册仓库已初始化");
    let node_repo_for_cleanup = node_repo.clone();
    let cleanup_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(120));
        loop {
            interval.tick().await;
            match node_repo_for_cleanup.cleanup_offline().await {
                Ok(count) if count > 0 => {
                    info!(deleted_count = count, "已清理离线节点");
                }
                Err(e) => {
                    tracing::error!("清理离线节点失败: {}", e);
                }
                _ => {}
            }
        }
    });
    info!("节点清理任务已启动");
    sync_authorization_expiry(&repo, &base_authorization, None).await;
    if let Err(error) = base_authorization
        .restore_active_package_if_needed(repo.get_pool())
        .await
    {
        tracing::warn!(
            "Failed to restore commercial base authorization runtime from database snapshot: {}",
            error
        );
    }
    let state = server::AppState {
        base_authorization,
        auth_state,
        repo,
        node_repo,
        config_repo,
        service_registry_repo,
        root_material_cache: Arc::new(Mutex::new(None)),
        root_material_refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
    };
    if let Err(error) = state.current_root_material_bundle().await {
        tracing::warn!(error = % error, "Root material cache warmup failed");
    } else {
        info!("Root material cache warmup completed");
    }
    let sync_repo = state.repo.clone();
    let sync_base_authorization = state.base_authorization.clone();
    let sync_root_material_cache = state.root_material_cache.clone();
    let app = server::create_app(state.clone());
    let addr = config.server.socket_addr()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("证书服务监听于 {}", addr);
    let sync_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            sync_authorization_expiry(
                &sync_repo,
                &sync_base_authorization,
                Some(&sync_root_material_cache),
            )
            .await;
        }
    });
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("Failed to install Ctrl+C handler: {}", e);
        } else {
            info!("收到关闭信号");
        }
        cleanup_handle.abort();
        sync_handle.abort();
        info!("已发送关闭信号给清理任务");
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(5), cleanup_handle).await;
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(5), sync_handle).await;
    })
    .await?;
    info!("证书服务关闭完成");
    Ok(())
}
