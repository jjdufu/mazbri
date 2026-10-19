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
        mazbri_core::config_watch::spawn_config_watch(
            (*platform.pool).clone(),
            platform.database,
            config_watch,
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
    }

    mazbri_service::run_tarpc_server!(
        addr,
        service,
        tls_acceptor,
        max_connections,
        |svc: AuthTarpcService, ch| BaseChannel::with_defaults(ch).execute(svc.serve())
    )
    .await
    .map_err(|e| anyhow::anyhow!("tarpc server error: {}", e))?;
    info!("认证服务关闭完成");
    Ok(())
}
