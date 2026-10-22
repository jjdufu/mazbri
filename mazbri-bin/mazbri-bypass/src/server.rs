use crate::service::MazbriBypassServiceImpl;
use anyhow::Result;
use mazbri_tls::ManagedCertificate;
use mazbri_upstream_contracts::bypass_http_proxy::bypass_http_proxy_service_server::BypassHttpProxyServiceServer;
use std::net::SocketAddr;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tracing::info;

pub async fn serve(
    addr: SocketAddr,
    service: MazbriBypassServiceImpl,
    cert: ManagedCertificate,
    max_connections: u32,
) -> Result<()> {
    let identity = Identity::from_pem(cert.cert_pem.as_bytes(), cert.key_pem.as_bytes());
    let client_ca = Certificate::from_pem(cert.ca_cert_pem.as_bytes());
    let tls_config = ServerTlsConfig::new()
        .identity(identity)
        .client_ca_root(client_ca);
    info!(
        "mazbri-bypass Egress HTTP/2 server listening on {} (mTLS mandatory, max connections: {}, contract=BYPASS-HTTP-PROXY)",
        addr, max_connections
    );
    Server::builder()
        .tls_config(tls_config)
        .map_err(|e| anyhow::anyhow!("failed to configure tonic TLS: {e}"))?
        .concurrency_limit_per_connection(max_connections as usize)
        .add_service(BypassHttpProxyServiceServer::new(service))
        .serve(addr)
        .await?;
    Ok(())
}
