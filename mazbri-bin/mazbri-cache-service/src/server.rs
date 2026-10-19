use crate::tarpc_service::CacheTarpcService;
use anyhow::Result;
use mazbri_cache::{CacheConfig, CacheService};
use mazbri_rpc::cache::CacheService as CacheRpcService;
use mazbri_tls::TlsAcceptor;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;
pub async fn serve(
    addr: SocketAddr,
    db: Arc<PgPool>,
    cache_config: CacheConfig,
    redis: mazbri_core::redis::RedisClient,
    tls_acceptor: Option<TlsAcceptor>,
    max_connections: u32,
) -> Result<()> {
    info!(
        "Starting tarpc cache server on {} (max connections: {})",
        addr, max_connections
    );
    let cache_service = CacheService::new(db, cache_config.clone(), redis).await?;
    let cache = Arc::clone(cache_service.cache());
    let ttl_manager = Arc::clone(cache_service.ttl_manager());
    let service = CacheTarpcService::new(cache, ttl_manager);
    mazbri_service::run_tarpc_server!(
        addr,
        service,
        tls_acceptor,
        max_connections,
        |svc: CacheTarpcService, ch| BaseChannel::with_defaults(ch).execute(svc.serve())
    )
    .await
    .map_err(|e| anyhow::anyhow!("tarpc server error: {}", e))?;
    info!("缓存服务关闭完成");
    Ok(())
}
