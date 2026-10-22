use crate::tarpc_service::CBProExtServiceImpl;
use anyhow::Result;
use mazbri_rpc::CBProExtService;
use mazbri_tls::TlsAcceptor;
use std::net::SocketAddr;
use tracing::info;

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn serve(
    addr: SocketAddr,
    service: CBProExtServiceImpl,
    tls_acceptor: Option<TlsAcceptor>,
    max_connections: u32,
) -> Result<()> {
    let acceptor = tls_acceptor.ok_or_else(|| anyhow::anyhow!("mTLS 需要 TLS 接收器"))?;
    info!(
        "CBPro-Ext tarpc server listening on {} (mTLS mandatory, max connections: {})",
        addr, max_connections
    );
    mazbri_service::run_tarpc_server!(
        addr,
        service,
        Some(acceptor),
        max_connections,
        |svc: CBProExtServiceImpl, ch| BaseChannel::with_defaults(ch).execute(svc.serve())
    )
    .await
    .map_err(|e| anyhow::anyhow!("tarpc server error: {e}"))?;
    Ok(())
}
