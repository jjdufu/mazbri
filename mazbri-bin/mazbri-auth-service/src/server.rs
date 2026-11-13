use crate::tarpc_service::AuthTarpcService;
use anyhow::Result;
use mazbri_rpc::auth::AuthService;
use mazbri_tls::TlsAcceptor;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

pub struct PlatformListen {
    pub pool: Arc<PgPool>,
    pub database: mazbri_core::DatabaseConfig,
}

pub async fn serve(
    addr: SocketAddr,
    db: Arc<PgPool>,
    auth_config: mazbri_auth::AuthServiceConfig,
    tls_acceptor: Option<TlsAcceptor>,
    max_connections: u32,
    platform: Option<PlatformListen>,
    config_watch: mazbri_core::config_watch::ConfigWatchConfig,
) -> Result<()> {
    info!(
        "Starting tarpc auth server on {} (max connections: {})",
        addr, max_connections
    );
    let service = if let Some(platform) = platform.as_ref() {
        AuthTarpcService::with_whitelist_db(db, platform.pool.clone(), auth_config)
    } else {
        tracing::warn!("platform_database 未配置，IP 白名单仍读认证库，无法监听平台库通知");
        AuthTarpcService::new(db, auth_config)
    };
    if let Some(platform) = platform {
        let whitelist = service.ip_whitelist();
        let validator = service.api_key_validator();
        let platform_pool = (*platform.pool).clone();
        let platform_db_cfg = platform.database;
        mazbri_core::config_watch::spawn_config_watch(
            platform_pool.clone(),
            platform_db_cfg.clone(),
            config_watch.clone(),
            mazbri_core::config_watch::AUTH_WHITELIST_VERSION_KEY,
            "auth-whitelist",
            move || {
                let whitelist = whitelist.clone();
                async move {
                    whitelist.invalidate_all();
                }
            },
        );
        info!("IP whitelist config watch started");
        if let Err(error) = mazbri_core::config_watch::ensure_config_version_seeded(
            &platform_pool,
            mazbri_core::config_watch::AUTH_API_KEY_VERSION_KEY,
        )
        .await
        {
            tracing::warn!(error = %error, "Failed to seed auth_api_key_version in route_sync_metadata");
        }
        mazbri_core::config_watch::spawn_config_watch(
            platform_pool,
            platform_db_cfg,
            config_watch,
            mazbri_core::config_watch::AUTH_API_KEY_VERSION_KEY,
            "auth-api-key",
            move || {
                let validator = validator.clone();
                async move {
                    validator.invalidate_all_validation_cache();
                }
            },
        );
        info!("API key validation cache config watch started");
    }

    mazbri_service::run_tarpc_server!(
        addr,
        service,
        tls_acceptor,
        max_connections,
        |svc: AuthTarpcService, ch| {
            mazbri_core::tarpc_middleware::trace::execute_traced(ch, svc.serve())
        }
    )
    .await
    .map_err(|e| anyhow::anyhow!("tarpc server error: {e}"))?;
    info!("认证服务关闭完成");
    Ok(())
}
