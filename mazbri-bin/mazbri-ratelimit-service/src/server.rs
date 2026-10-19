use crate::tarpc_service::RateLimitTarpcService;
use anyhow::Result;
use mazbri_rpc::ratelimit::RateLimitService;
use mazbri_tls::TlsAcceptor;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

pub async fn serve_with_config(
    addr: SocketAddr,
    db: Arc<PgPool>,
    tls_acceptor: Option<TlsAcceptor>,
    audit_enabled: bool,
    max_connections: u32,
) -> Result<()> {
    info!(
        "Starting tarpc rate limit server on {} (max connections: {})",
        addr, max_connections
    );
    let service = RateLimitTarpcService::new(db, audit_enabled);
    mazbri_service::run_tarpc_server!(
        addr,
        service,
        tls_acceptor,
        max_connections,
        |svc: RateLimitTarpcService, ch| BaseChannel::with_defaults(ch).execute(svc.serve())
    )
    .await
    .map_err(|e| anyhow::anyhow!("tarpc server error: {}", e))?;
    info!("流控服务关闭完成");
    Ok(())
}
