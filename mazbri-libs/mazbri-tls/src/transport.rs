use crate::{ManagedCertificate, TlsConfig, TlsError, TlsResult};
use rustls_pemfile::{certs, private_key};
use std::net::IpAddr;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::{
    TlsAcceptor as RustlsAcceptor, TlsConnector as RustlsConnector,
    client::TlsStream as ClientTlsStream, server::TlsStream,
};
use tracing::{debug, warn};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectTarget {
    pub socket_addr: SocketAddr,
    pub server_name: Option<String>,
    pub display_addr: String,
}
#[derive(Clone)]
pub struct TlsAcceptor {
    inner: Arc<RustlsAcceptor>,
}
impl TlsAcceptor {
    pub fn new(cert: &ManagedCertificate, config: &TlsConfig) -> TlsResult<Self> {
        let server_config = build_server_config(cert, config)?;
        let acceptor = RustlsAcceptor::from(server_config);
        Ok(Self {
            inner: Arc::new(acceptor),
        })
    }
    pub async fn accept<S>(&self, stream: S) -> TlsResult<TlsStream<S>>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        self.inner
            .accept(stream)
            .await
            .map_err(|e| TlsError::HandshakeFailed(e.to_string()))
    }
}
#[derive(Clone)]
pub struct TlsConnector {
    inner: Arc<RustlsConnector>,
    domain: rustls::pki_types::ServerName<'static>,
}
impl TlsConnector {
    pub fn new(
        ca_cert_pem: &str,
        client_cert: Option<&ManagedCertificate>,
        config: &TlsConfig,
    ) -> TlsResult<Self> {
        let client_config = build_client_config(ca_cert_pem, client_cert, config)?;
        let connector = RustlsConnector::from(client_config);
        let domain_str = config
            .server_name
            .as_deref()
            .unwrap_or(&config.service_identity);
        let domain = rustls::pki_types::ServerName::try_from(domain_str)
            .map_err(|e| {
                TlsError::Config(format!(
                    "Invalid server name '{}': {}. service_identity='{}'",
                    domain_str, e, config.service_identity
                ))
            })?
            .to_owned();
        Ok(Self {
            inner: Arc::new(connector),
            domain,
        })
    }
    pub fn with_domain(
        ca_cert_pem: &str,
        client_cert: Option<&ManagedCertificate>,
        config: &TlsConfig,
        domain: &str,
    ) -> TlsResult<Self> {
        let client_config = build_client_config(ca_cert_pem, client_cert, config)?;
        let connector = RustlsConnector::from(client_config);
        let server_name = rustls::pki_types::ServerName::try_from(domain)
            .map_err(|e| TlsError::Config(format!("Invalid server name '{}': {}", domain, e)))?
            .to_owned();
        Ok(Self {
            inner: Arc::new(connector),
            domain: server_name,
        })
    }
    pub async fn connect(&self, addr: SocketAddr) -> TlsResult<ClientTlsStream<TcpStream>> {
        self.connect_with_server_name(addr, None).await
    }
    pub async fn connect_with_server_name(
        &self,
        addr: SocketAddr,
        server_name: Option<&str>,
    ) -> TlsResult<ClientTlsStream<TcpStream>> {
        debug!(addr = % addr, "Connecting to TLS server");
        let tcp_stream = TcpStream::connect(addr)
            .await
            .map_err(|e| TlsError::HandshakeFailed(format!("TCP connect failed: {}", e)))?;
        let domain = if let Some(server_name) = server_name {
            rustls::pki_types::ServerName::try_from(server_name)
                .map_err(|e| {
                    TlsError::Config(format!("Invalid server name '{}': {}", server_name, e))
                })?
                .to_owned()
        } else {
            self.domain.clone()
        };
        self.inner
            .connect(domain, tcp_stream)
            .await
            .map_err(|e| TlsError::HandshakeFailed(e.to_string()))
    }
}
pub async fn resolve_connect_target(
    endpoint: &str,
    explicit_server_name: Option<&str>,
) -> TlsResult<ConnectTarget> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err(TlsError::Config("Endpoint must not be empty".to_string()));
    }
    let url = if endpoint.contains("://") {
        url::Url::parse(endpoint)
    } else {
        url::Url::parse(&format!("tcp://{endpoint}"))
    }
    .map_err(|e| TlsError::Config(format!("Invalid endpoint '{}': {}", endpoint, e)))?;
    let host = url
        .host_str()
        .ok_or_else(|| TlsError::Config(format!("Endpoint '{}' missing host", endpoint)))?;
    let port = url
        .port()
        .ok_or_else(|| TlsError::Config(format!("Endpoint '{}' missing port", endpoint)))?;
    let display_addr = format!("{host}:{port}");
    let mut addrs = tokio::net::lookup_host((host, port)).await.map_err(|e| {
        TlsError::HandshakeFailed(format!("Failed to resolve '{}': {}", display_addr, e))
    })?;
    let socket_addr = addrs.next().ok_or_else(|| {
        TlsError::HandshakeFailed(format!("No resolved addresses for '{}'", display_addr))
    })?;
    let query_server_name = url
        .query_pairs()
        .find(|(key, _)| key == "server_name")
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.is_empty());
    let server_name = explicit_server_name
        .map(ToOwned::to_owned)
        .or(query_server_name)
        .or_else(|| {
            if host.parse::<IpAddr>().is_ok() {
                None
            } else {
                Some(host.to_string())
            }
        });
    Ok(ConnectTarget {
        socket_addr,
        server_name,
        display_addr,
    })
}
fn init_crypto_provider() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}
fn build_server_config(
    cert: &ManagedCertificate,
    config: &TlsConfig,
) -> TlsResult<Arc<rustls::ServerConfig>> {
    init_crypto_provider();
    use rustls::ServerConfig;
    use rustls::server::WebPkiClientVerifier;
    use std::io::Cursor;
    let cert_chain: Vec<rustls::pki_types::CertificateDer> = {
        use rustls_pemfile::certs;
        use std::io::Cursor;
        let certs_vec: Vec<_> = certs(&mut Cursor::new(cert.cert_pem.as_bytes())).collect();
        let mut valid_certs = Vec::new();
        let mut error_count = 0;
        for (idx, result) in certs_vec.into_iter().enumerate() {
            match result {
                Ok(cert) => valid_certs.push(cert),
                Err(e) => {
                    error_count += 1;
                    if error_count <= 2 {
                        warn!(
                            index = idx, error = % e,
                            "Failed to parse server certificate, skipping"
                        );
                    }
                }
            }
        }
        if error_count > 2 {
            warn!(
                "... and {} more server certificate parse errors",
                error_count - 2
            );
        }
        valid_certs
    };
    if cert_chain.is_empty() {
        return Err(TlsError::InvalidCertificate("PEM 中未找到证书".to_string()));
    }
    let key = private_key(&mut Cursor::new(cert.key_pem.as_bytes()))
        .map_err(|e| TlsError::InvalidCertificate(format!("Failed to parse key: {}", e)))?
        .ok_or_else(|| TlsError::InvalidCertificate("PEM 中未找到私钥".to_string()))?;
    let server_config = if config.verify_peer {
        let ca_certs: Vec<rustls::pki_types::CertificateDer> = {
            let certs_vec: Vec<_> = certs(&mut Cursor::new(cert.ca_cert_pem.as_bytes())).collect();
            let mut valid_certs = Vec::new();
            let mut error_count = 0;
            for (idx, result) in certs_vec.into_iter().enumerate() {
                match result {
                    Ok(cert) => valid_certs.push(cert),
                    Err(e) => {
                        error_count += 1;
                        if error_count <= 2 {
                            warn!(
                                index = idx, error = % e,
                                "Failed to parse CA certificate, skipping"
                            );
                        }
                    }
                }
            }
            if error_count > 2 {
                warn!(
                    "... and {} more CA certificate parse errors",
                    error_count - 2
                );
            }
            valid_certs
        };
        if ca_certs.is_empty() {
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(cert_chain, key)
                .map_err(|e| TlsError::Config(format!("Failed to build server config: {}", e)))?
        } else {
            let mut root_store = rustls::RootCertStore::empty();
            for cert in ca_certs {
                root_store
                    .add(cert)
                    .map_err(|e| TlsError::Config(format!("Failed to add CA cert: {}", e)))?;
            }
            let verifier = WebPkiClientVerifier::builder(root_store.into())
                .build()
                .map_err(|e| TlsError::Config(format!("Failed to build client verifier: {}", e)))?;
            ServerConfig::builder()
                .with_client_cert_verifier(verifier)
                .with_single_cert(cert_chain, key)
                .map_err(|e| TlsError::Config(format!("Failed to build server config: {}", e)))?
        }
    } else {
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, key)
            .map_err(|e| TlsError::Config(format!("Failed to build server config: {}", e)))?
    };
    Ok(Arc::new(server_config))
}
fn build_client_config(
    ca_cert_pem: &str,
    client_cert: Option<&ManagedCertificate>,
    _config: &TlsConfig,
) -> TlsResult<Arc<rustls::ClientConfig>> {
    init_crypto_provider();
    use rustls::ClientConfig;
    use std::io::Cursor;
    let ca_certs: Vec<rustls::pki_types::CertificateDer> = {
        let certs_vec: Vec<_> = certs(&mut Cursor::new(ca_cert_pem.as_bytes())).collect();
        let mut valid_certs = Vec::new();
        let mut error_count = 0;
        for (idx, result) in certs_vec.into_iter().enumerate() {
            match result {
                Ok(cert) => valid_certs.push(cert),
                Err(e) => {
                    error_count += 1;
                    if error_count <= 2 {
                        warn!(
                            index = idx, error = % e,
                            "Failed to parse client CA certificate, skipping"
                        );
                    }
                }
            }
        }
        if error_count > 2 {
            warn!(
                "... and {} more client CA certificate parse errors",
                error_count - 2
            );
        }
        valid_certs
    };
    if ca_certs.is_empty() {
        return Err(TlsError::InvalidCertificate(
            "PEM 中未找到 CA 证书".to_string(),
        ));
    }
    let mut root_store = rustls::RootCertStore::empty();
    for cert in ca_certs {
        root_store
            .add(cert)
            .map_err(|e| TlsError::Config(format!("Failed to add CA cert: {}", e)))?;
    }
    let builder = ClientConfig::builder().with_root_certificates(root_store);
    let client_config = if let Some(cert) = client_cert {
        let cert_chain: Vec<rustls::pki_types::CertificateDer> = {
            let certs_vec: Vec<_> = certs(&mut Cursor::new(cert.cert_pem.as_bytes())).collect();
            let mut valid_certs = Vec::new();
            let mut error_count = 0;
            for (idx, result) in certs_vec.into_iter().enumerate() {
                match result {
                    Ok(cert) => valid_certs.push(cert),
                    Err(e) => {
                        error_count += 1;
                        if error_count <= 2 {
                            warn!(
                                index = idx, error = % e,
                                "Failed to parse client certificate, skipping"
                            );
                        }
                    }
                }
            }
            if error_count > 2 {
                warn!(
                    "... and {} more client certificate parse errors",
                    error_count - 2
                );
            }
            valid_certs
        };
        if cert_chain.is_empty() {
            return Err(TlsError::InvalidCertificate(
                "PEM 中未找到客户端证书".to_string(),
            ));
        }
        let key = private_key(&mut Cursor::new(cert.key_pem.as_bytes()))
            .map_err(|e| {
                TlsError::InvalidCertificate(format!("Failed to parse client key: {}", e))
            })?
            .ok_or_else(|| TlsError::InvalidCertificate("未找到客户端私钥".to_string()))?;
        builder
            .with_client_auth_cert(cert_chain, key)
            .map_err(|e| {
                TlsError::Config(format!("Failed to build client config with cert: {}", e))
            })?
    } else {
        builder.with_no_client_auth()
    };
    Ok(Arc::new(client_config))
}
