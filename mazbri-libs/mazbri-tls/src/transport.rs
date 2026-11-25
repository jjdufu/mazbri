use crate::{ManagedCertHandle, ManagedCertificate, TlsConfig, TlsError, TlsResult};
use arc_swap::{ArcSwap, ArcSwapOption};
use mazbri_core::ServiceName;
use rustls::client::ResolvesClientCert;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::server::{ClientHello, ResolvesServerCert, WebPkiClientVerifier};
use rustls::sign::CertifiedKey;
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use rustls_pemfile::{certs, private_key};
use std::collections::HashSet;
use std::io::Cursor;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::{
    TlsAcceptor as RustlsAcceptor, TlsConnector as RustlsConnector,
    client::TlsStream as ClientTlsStream, server::TlsStream,
};
use tracing::{debug, warn};
use x509_parser::extensions::GeneralName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectTarget {
    pub socket_addr: SocketAddr,
    pub server_name: Option<String>,
    pub display_addr: String,
}

/// Enable `TCP_NODELAY` on an internal tarpc socket. Failure is logged and ignored
/// so a single platform quirk cannot abort the connection.
pub(crate) fn enable_tcp_nodelay(stream: &TcpStream, peer: impl std::fmt::Display) {
    if let Err(err) = stream.set_nodelay(true) {
        warn!(error = %err, peer = %peer, "failed to set TCP_NODELAY on tarpc socket");
    }
}

/// 规范化 X.509 序列号字节（剥离正整数前导 `0x00` 符号填充字节）。
#[must_use]
pub fn normalize_serial_bytes(raw: &[u8]) -> Vec<u8> {
    let trimmed = raw
        .iter()
        .position(|&b| b != 0)
        .map_or(&[0u8][..], |idx| &raw[idx..]);
    trimmed.to_vec()
}

/// 基于 `ArcSwap` 的零停机动态证书解析器，同时实现服务端与客户端证书热替换。
#[derive(Debug, Clone)]
pub struct DynamicCertResolver {
    certified_key: Arc<ArcSwapOption<CertifiedKey>>,
}

impl DynamicCertResolver {
    #[must_use]
    pub fn new(handle: &ManagedCertHandle) -> Self {
        Self {
            certified_key: handle.certified_key_swap(),
        }
    }
}

impl ResolvesServerCert for DynamicCertResolver {
    fn resolve(&self, _client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        self.certified_key.load_full()
    }
}

impl ResolvesClientCert for DynamicCertResolver {
    fn resolve(
        &self,
        _root_hint_subjects: &[&[u8]],
        _sigschemes: &[SignatureScheme],
    ) -> Option<Arc<CertifiedKey>> {
        self.certified_key.load_full()
    }

    fn has_certs(&self) -> bool {
        self.certified_key.load().is_some()
    }
}

fn check_end_entity_not_revoked(
    end_entity: &CertificateDer<'_>,
    revoked: &ArcSwap<HashSet<Vec<u8>>>,
) -> Result<(), rustls::Error> {
    let (_, parsed) = x509_parser::parse_x509_certificate(end_entity.as_ref())
        .map_err(|_| rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding))?;
    let serial = normalize_serial_bytes(parsed.raw_serial());
    if revoked.load().contains(&serial) {
        return Err(rustls::Error::InvalidCertificate(
            rustls::CertificateError::Revoked,
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct RevocationClientCertVerifier {
    inner: Arc<dyn ClientCertVerifier>,
    revoked: Arc<ArcSwap<HashSet<Vec<u8>>>>,
}

impl ClientCertVerifier for RevocationClientCertVerifier {
    fn offer_client_auth(&self) -> bool {
        self.inner.offer_client_auth()
    }

    fn client_auth_mandatory(&self) -> bool {
        self.inner.client_auth_mandatory()
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        self.inner.root_hint_subjects()
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        check_end_entity_not_revoked(end_entity, &self.revoked)?;
        self.inner
            .verify_client_cert(end_entity, intermediates, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

#[derive(Debug)]
struct RevocationServerCertVerifier {
    inner: Arc<dyn ServerCertVerifier>,
    revoked: Arc<ArcSwap<HashSet<Vec<u8>>>>,
}

impl ServerCertVerifier for RevocationServerCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        check_end_entity_not_revoked(end_entity, &self.revoked)?;
        self.inner
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// 内部服务调用者身份对端证书 DNS SAN 访问控制矩阵（Peer SAN ACL）。
///
/// - `mazbri-auth` / `mazbri-ratelimit` / `mazbri-cache` / `mazbri-bypass` /
///   `mazbri-cbpro-ext` / `mazbri-billing`：仅允许 `mazbri-gateway`
/// - `mazbri-cbpro`：仅允许 `mazbri-gateway` 与 `mazbri-cbpro-ext`
///
/// # Errors
///
/// 对端缺失证书、证书不可解析、未携带合法调用方 SAN 或混入越权服务身份时返回错误。
pub fn verify_peer_san_acl(
    service_identity: &str,
    peer_certs: Option<&[CertificateDer<'_>]>,
) -> TlsResult<()> {
    let allowed: &[&str] = match service_identity.trim() {
        "mazbri-auth"
        | "mazbri-ratelimit"
        | "mazbri-cache"
        | "mazbri-bypass"
        | "mazbri-cbpro-ext"
        | "mazbri-billing" => &["mazbri-gateway"],
        "mazbri-cbpro" => &["mazbri-gateway", "mazbri-cbpro-ext"],
        other => {
            return Err(TlsError::HandshakeFailed(format!(
                "peer SAN ACL denied: target service '{other}' does not accept mTLS peers"
            )));
        }
    };

    let leaf = peer_certs
        .and_then(|certs| certs.first())
        .ok_or_else(|| TlsError::HandshakeFailed("missing peer leaf certificate".to_string()))?;
    let (_, parsed) = x509_parser::parse_x509_certificate(leaf.as_ref()).map_err(|e| {
        TlsError::HandshakeFailed(format!("failed to parse peer X.509 certificate: {e}"))
    })?;
    let san_ext = parsed
        .subject_alternative_name()
        .map_err(|e| TlsError::HandshakeFailed(format!("invalid peer SAN extension: {e}")))?
        .ok_or_else(|| {
            TlsError::HandshakeFailed("peer certificate has no SAN extension".to_string())
        })?;

    let mut matched_allowed = false;
    for general_name in &san_ext.value.general_names {
        if let GeneralName::DNSName(raw_dns) = general_name {
            let dns = raw_dns.trim().trim_end_matches('.').to_ascii_lowercase();
            if allowed.contains(&dns.as_str()) {
                matched_allowed = true;
            } else if let Ok(peer_svc) = dns.parse::<ServiceName>() {
                return Err(TlsError::HandshakeFailed(format!(
                    "peer service '{}' is not authorized to call '{service_identity}'",
                    peer_svc.as_str()
                )));
            }
        }
    }

    if !matched_allowed {
        return Err(TlsError::HandshakeFailed(format!(
            "peer certificate lacks authorized DNS SAN for '{service_identity}' (allowed: {allowed:?})"
        )));
    }
    Ok(())
}

#[derive(Clone)]
pub struct TlsAcceptor {
    inner: Arc<RustlsAcceptor>,
    service_identity: String,
    verify_peer: bool,
}

impl TlsAcceptor {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn new(cert: &ManagedCertificate, config: &TlsConfig) -> TlsResult<Self> {
        let server_config = build_server_config(cert, config)?;
        let acceptor = RustlsAcceptor::from(server_config);
        Ok(Self {
            inner: Arc::new(acceptor),
            service_identity: config.service_identity.clone(),
            verify_peer: config.verify_peer,
        })
    }

    /// 构造绑定 `DynamicCertResolver` 与吊销检查器的零停机动态 `TlsAcceptor`。
    ///
    /// # Errors
    ///
    /// 句柄尚未加载有效证书或 CA 配置非法时返回错误。
    pub fn from_cert_handle(handle: &ManagedCertHandle, config: &TlsConfig) -> TlsResult<Self> {
        let server_config = build_dynamic_server_config(handle, config, Vec::new())?;
        Ok(Self {
            inner: Arc::new(RustlsAcceptor::from(server_config)),
            service_identity: config.service_identity.clone(),
            verify_peer: config.verify_peer,
        })
    }

    /// 构造显式声明 `ALPN = ["h2"]` 的动态 `TlsAcceptor`（供 Tonic gRPC / HTTP2 mTLS 使用）。
    ///
    /// # Errors
    ///
    /// 句柄尚未加载有效证书或 CA 配置非法时返回错误。
    pub fn new_h2(handle: &ManagedCertHandle, config: &TlsConfig) -> TlsResult<Self> {
        let server_config =
            build_dynamic_server_config(handle, config, vec![b"h2".to_vec()])?;
        Ok(Self {
            inner: Arc::new(RustlsAcceptor::from(server_config)),
            service_identity: config.service_identity.clone(),
            verify_peer: config.verify_peer,
        })
    }

    #[must_use]
    pub fn service_identity(&self) -> &str {
        &self.service_identity
    }

    #[must_use]
    pub const fn verify_peer(&self) -> bool {
        self.verify_peer
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
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
    domain: ServerName<'static>,
}

impl TlsConnector {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn new(
        ca_cert_pem: &str,
        client_cert: Option<&ManagedCertificate>,
        config: &TlsConfig,
    ) -> TlsResult<Self> {
        let client_config = build_client_config(ca_cert_pem, client_cert, config)?;
        let connector = RustlsConnector::from(client_config);
        let domain_str = config.service_identity.as_str();
        let domain = ServerName::try_from(domain_str)
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

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn with_domain(
        ca_cert_pem: &str,
        client_cert: Option<&ManagedCertificate>,
        config: &TlsConfig,
        domain: &str,
    ) -> TlsResult<Self> {
        let client_config = build_client_config(ca_cert_pem, client_cert, config)?;
        let connector = RustlsConnector::from(client_config);
        let server_name = ServerName::try_from(domain)
            .map_err(|e| TlsError::Config(format!("Invalid server name '{domain}': {e}")))?
            .to_owned();
        Ok(Self {
            inner: Arc::new(connector),
            domain: server_name,
        })
    }

    /// 构造绑定 `DynamicCertResolver` 与吊销检查器的动态客户端连接器。
    ///
    /// # Errors
    ///
    /// 句柄无有效证书、CA 为空或域名非法时返回错误。
    pub fn from_cert_handle(handle: &ManagedCertHandle, config: &TlsConfig) -> TlsResult<Self> {
        Self::from_cert_handle_with_domain(handle, config, &config.service_identity)
    }

    /// 构造指定目标 SNI 域名的动态客户端连接器。
    ///
    /// # Errors
    ///
    /// 句柄无有效证书、CA 为空或域名非法时返回错误。
    pub fn from_cert_handle_with_domain(
        handle: &ManagedCertHandle,
        config: &TlsConfig,
        domain: &str,
    ) -> TlsResult<Self> {
        let client_config = build_dynamic_client_config(handle, config, Vec::new())?;
        let server_name = ServerName::try_from(domain)
            .map_err(|e| TlsError::Config(format!("Invalid server name '{domain}': {e}")))?
            .to_owned();
        Ok(Self {
            inner: Arc::new(RustlsConnector::from(client_config)),
            domain: server_name,
        })
    }

    /// 构造显式声明 `ALPN = ["h2"]` 的动态客户端连接器（供 Gateway → Bypass HTTP/2 gRPC 使用）。
    ///
    /// # Errors
    ///
    /// 句柄无有效证书、CA 为空或域名非法时返回错误。
    pub fn new_h2(
        handle: &ManagedCertHandle,
        config: &TlsConfig,
        domain: &str,
    ) -> TlsResult<Self> {
        let client_config = build_dynamic_client_config(handle, config, vec![b"h2".to_vec()])?;
        let server_name = ServerName::try_from(domain)
            .map_err(|e| TlsError::Config(format!("Invalid server name '{domain}': {e}")))?
            .to_owned();
        Ok(Self {
            inner: Arc::new(RustlsConnector::from(client_config)),
            domain: server_name,
        })
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn connect(&self, addr: SocketAddr) -> TlsResult<ClientTlsStream<TcpStream>> {
        self.connect_with_server_name(addr, None).await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn connect_with_server_name(
        &self,
        addr: SocketAddr,
        server_name: Option<&str>,
    ) -> TlsResult<ClientTlsStream<TcpStream>> {
        debug!(addr = %addr, "Connecting to TLS server");
        let tcp_stream = TcpStream::connect(addr)
            .await
            .map_err(|e| TlsError::HandshakeFailed(format!("TCP connect failed: {e}")))?;
        enable_tcp_nodelay(&tcp_stream, addr);
        let domain = if let Some(server_name) = server_name {
            ServerName::try_from(server_name)
                .map_err(|e| TlsError::Config(format!("Invalid server name '{server_name}': {e}")))?
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

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn resolve_connect_target(endpoint: &str) -> TlsResult<ConnectTarget> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err(TlsError::Config("Endpoint must not be empty".to_string()));
    }
    let url = if endpoint.contains("://") {
        url::Url::parse(endpoint)
    } else {
        url::Url::parse(&format!("tcp://{endpoint}"))
    }
    .map_err(|e| TlsError::Config(format!("Invalid endpoint '{endpoint}': {e}")))?;
    let host = url
        .host_str()
        .ok_or_else(|| TlsError::Config(format!("Endpoint '{endpoint}' missing host")))?;
    let port = url
        .port()
        .ok_or_else(|| TlsError::Config(format!("Endpoint '{endpoint}' missing port")))?;
    let display_addr = format!("{host}:{port}");
    let mut addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| TlsError::Resolve(format!("Failed to resolve '{display_addr}': {e}")))?;
    let socket_addr = addrs
        .next()
        .ok_or_else(|| TlsError::Resolve(format!("No resolved addresses for '{display_addr}'")))?;
    let server_name = if host.parse::<IpAddr>().is_ok() {
        None
    } else {
        Some(host.to_string())
    };
    Ok(ConnectTarget {
        socket_addr,
        server_name,
        display_addr,
    })
}

pub(crate) fn init_crypto_provider() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

fn parse_pem_certs(pem: &str, role: &str) -> Vec<CertificateDer<'static>> {
    let certs_vec: Vec<_> = certs(&mut Cursor::new(pem.as_bytes())).collect();
    let mut valid_certs = Vec::new();
    let mut error_count = 0usize;
    for (idx, result) in certs_vec.into_iter().enumerate() {
        match result {
            Ok(cert) => valid_certs.push(cert),
            Err(e) => {
                error_count += 1;
                if error_count <= 2 {
                    warn!(
                        index = idx, error = %e, role,
                        "Failed to parse certificate PEM entry, skipping"
                    );
                }
            }
        }
    }
    if error_count > 2 {
        warn!(
            role,
            remaining = error_count - 2,
            "Additional certificate PEM parse errors suppressed"
        );
    }
    valid_certs
}

pub(crate) fn build_certified_key(cert_pem: &str, key_pem: &str) -> TlsResult<Arc<CertifiedKey>> {
    init_crypto_provider();
    let cert_chain = parse_pem_certs(cert_pem, "leaf");
    if cert_chain.is_empty() {
        return Err(TlsError::InvalidCertificate("PEM 中未找到证书".to_string()));
    }
    let key = private_key(&mut Cursor::new(key_pem.as_bytes()))
        .map_err(|e| TlsError::InvalidCertificate(format!("Failed to parse key: {e}")))?
        .ok_or_else(|| TlsError::InvalidCertificate("PEM 中未找到私钥".to_string()))?;
    let signing_key = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key)
        .map_err(|e| TlsError::InvalidCertificate(format!("Unsupported private key: {e}")))?;
    Ok(Arc::new(CertifiedKey::new(cert_chain, signing_key)))
}

fn build_root_store(ca_cert_pem: &str) -> TlsResult<rustls::RootCertStore> {
    let ca_certs = parse_pem_certs(ca_cert_pem, "ca");
    if ca_certs.is_empty() {
        return Err(TlsError::InvalidCertificate(
            "PEM 中未找到有效的 CA 证书".to_string(),
        ));
    }
    let mut root_store = rustls::RootCertStore::empty();
    for cert in ca_certs {
        root_store
            .add(cert)
            .map_err(|e| TlsError::Config(format!("Failed to add CA cert: {e}")))?;
    }
    Ok(root_store)
}

fn build_dynamic_server_config(
    handle: &ManagedCertHandle,
    config: &TlsConfig,
    alpn_protocols: Vec<Vec<u8>>,
) -> TlsResult<Arc<rustls::ServerConfig>> {
    init_crypto_provider();
    if handle.certified_key_swap().load().is_none() {
        return Err(TlsError::InvalidCertificate(
            "ManagedCertHandle 中尚未装载有效证书与私钥".to_string(),
        ));
    }
    let resolver = Arc::new(DynamicCertResolver::new(handle));
    let mut server_config = if config.verify_peer {
        let ca_pem = handle.ca_cert_pem().ok_or_else(|| {
            TlsError::InvalidCertificate("开启 verify_peer 时必须提供有效的 CA 证书".to_string())
        })?;
        let root_store = build_root_store(&ca_pem)?;
        let webpki_verifier = WebPkiClientVerifier::builder(Arc::new(root_store))
            .build()
            .map_err(|e| TlsError::Config(format!("Failed to build client verifier: {e}")))?;
        let verifier = Arc::new(RevocationClientCertVerifier {
            inner: webpki_verifier,
            revoked: handle.revoked_serials_swap(),
        });
        rustls::ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_cert_resolver(resolver)
    } else {
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(resolver)
    };
    server_config.alpn_protocols = alpn_protocols;
    server_config.session_storage = Arc::new(rustls::server::NoServerSessionStorage {});
    server_config.send_tls13_tickets = 0;
    Ok(Arc::new(server_config))
}

fn build_dynamic_client_config(
    handle: &ManagedCertHandle,
    _config: &TlsConfig,
    alpn_protocols: Vec<Vec<u8>>,
) -> TlsResult<Arc<rustls::ClientConfig>> {
    init_crypto_provider();
    if handle.certified_key_swap().load().is_none() {
        return Err(TlsError::InvalidCertificate(
            "ManagedCertHandle 中尚未装载有效客户端证书与私钥".to_string(),
        ));
    }
    let ca_pem = handle.ca_cert_pem().ok_or_else(|| {
        TlsError::InvalidCertificate("ManagedCertHandle 中缺少 CA 证书".to_string())
    })?;
    let root_store = build_root_store(&ca_pem)?;
    let webpki_verifier = rustls::client::WebPkiServerVerifier::builder(Arc::new(root_store))
        .build()
        .map_err(|e| TlsError::Config(format!("Failed to build server verifier: {e}")))?;
    let verifier = Arc::new(RevocationServerCertVerifier {
        inner: webpki_verifier,
        revoked: handle.revoked_serials_swap(),
    });
    let resolver = Arc::new(DynamicCertResolver::new(handle));
    let mut client_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_cert_resolver(resolver);
    client_config.alpn_protocols = alpn_protocols;
    client_config.resumption = rustls::client::Resumption::disabled();
    Ok(Arc::new(client_config))
}

fn build_server_config(
    cert: &ManagedCertificate,
    config: &TlsConfig,
) -> TlsResult<Arc<rustls::ServerConfig>> {
    init_crypto_provider();
    let cert_chain = parse_pem_certs(&cert.cert_pem, "server");
    if cert_chain.is_empty() {
        return Err(TlsError::InvalidCertificate("PEM 中未找到证书".to_string()));
    }
    let key = private_key(&mut Cursor::new(cert.key_pem.as_bytes()))
        .map_err(|e| TlsError::InvalidCertificate(format!("Failed to parse key: {e}")))?
        .ok_or_else(|| TlsError::InvalidCertificate("PEM 中未找到私钥".to_string()))?;
    let server_config = if config.verify_peer {
        let root_store = build_root_store(&cert.ca_cert_pem)?;
        let verifier = WebPkiClientVerifier::builder(Arc::new(root_store))
            .build()
            .map_err(|e| TlsError::Config(format!("Failed to build client verifier: {e}")))?;
        rustls::ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(cert_chain, key)
            .map_err(|e| TlsError::Config(format!("Failed to build server config: {e}")))?
    } else {
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, key)
            .map_err(|e| TlsError::Config(format!("Failed to build server config: {e}")))?
    };
    Ok(Arc::new(server_config))
}

fn build_client_config(
    ca_cert_pem: &str,
    client_cert: Option<&ManagedCertificate>,
    _config: &TlsConfig,
) -> TlsResult<Arc<rustls::ClientConfig>> {
    init_crypto_provider();
    let root_store = build_root_store(ca_cert_pem)?;
    let builder = rustls::ClientConfig::builder().with_root_certificates(root_store);
    let client_config = if let Some(cert) = client_cert {
        let cert_chain = parse_pem_certs(&cert.cert_pem, "client");
        if cert_chain.is_empty() {
            return Err(TlsError::InvalidCertificate(
                "PEM 中未找到客户端证书".to_string(),
            ));
        }
        let key = private_key(&mut Cursor::new(cert.key_pem.as_bytes()))
            .map_err(|e| TlsError::InvalidCertificate(format!("Failed to parse client key: {e}")))?
            .ok_or_else(|| TlsError::InvalidCertificate("未找到客户端私钥".to_string()))?;
        builder
            .with_client_auth_cert(cert_chain, key)
            .map_err(|e| {
                TlsError::Config(format!("Failed to build client config with cert: {e}"))
            })?
    } else {
        builder.with_no_client_auth()
    };
    Ok(Arc::new(client_config))
}

#[cfg(test)]
pub(crate) mod test_pki {
    use crate::ManagedCertificate;
    use chrono::{TimeDelta, Utc};
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};
    use zeroize::Zeroizing;

    pub(crate) struct TestCa {
        dir: PathBuf,
        ca_cert_pem: String,
        next_serial: AtomicU64,
    }

    impl Drop for TestCa {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn run_openssl(args: &[&str]) {
        let out = Command::new("openssl")
            .args(args)
            .output()
            .expect("spawn openssl");
        assert!(
            out.status.success(),
            "openssl {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    impl TestCa {
        pub(crate) fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("mazbri-tls-test-{}", uuid::Uuid::now_v7()));
            std::fs::create_dir_all(&dir).expect("mkdir");
            let ca_key = dir.join("ca.key.pem");
            let ca_cert = dir.join("ca.cert.pem");
            let ca_ext = dir.join("ca.cnf");
            std::fs::write(
                &ca_ext,
                "[req]\ndistinguished_name=dn\nx509_extensions=v3_ca\nprompt=no\n[dn]\nCN=Mazbri Test CA\n[v3_ca]\nbasicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign\n",
            )
            .expect("write ca.cnf");
            run_openssl(&[
                "genpkey",
                "-algorithm",
                "EC",
                "-pkeyopt",
                "ec_paramgen_curve:P-256",
                "-out",
                ca_key.to_str().expect("path"),
            ]);
            run_openssl(&[
                "req",
                "-new",
                "-x509",
                "-key",
                ca_key.to_str().expect("path"),
                "-out",
                ca_cert.to_str().expect("path"),
                "-days",
                "30",
                "-config",
                ca_ext.to_str().expect("path"),
                "-extensions",
                "v3_ca",
            ]);
            let ca_cert_pem = std::fs::read_to_string(&ca_cert).expect("read ca cert");
            Self {
                dir,
                ca_cert_pem,
                next_serial: AtomicU64::new(1000),
            }
        }

        pub(crate) fn issue_leaf(&self, subject: &str, sans: &[&str]) -> ManagedCertificate {
            let serial = self.next_serial.fetch_add(1, Ordering::Relaxed);
            let key_path = self.dir.join(format!("leaf-{serial}.key.pem"));
            let csr_path = self.dir.join(format!("leaf-{serial}.csr.pem"));
            let cert_path = self.dir.join(format!("leaf-{serial}.cert.pem"));
            let ext_path = self.dir.join(format!("leaf-{serial}.ext"));
            let san_list = sans
                .iter()
                .map(|s| format!("DNS:{s}"))
                .collect::<Vec<_>>()
                .join(",");
            std::fs::write(
                &ext_path,
                format!("basicConstraints=CA:FALSE\nsubjectAltName={san_list}\n"),
            )
            .expect("write ext");
            run_openssl(&[
                "genpkey",
                "-algorithm",
                "EC",
                "-pkeyopt",
                "ec_paramgen_curve:P-256",
                "-out",
                key_path.to_str().expect("path"),
            ]);
            run_openssl(&[
                "req",
                "-new",
                "-key",
                key_path.to_str().expect("path"),
                "-subj",
                &format!("/CN={subject}"),
                "-out",
                csr_path.to_str().expect("path"),
            ]);
            run_openssl(&[
                "x509",
                "-req",
                "-in",
                csr_path.to_str().expect("path"),
                "-CA",
                self.dir.join("ca.cert.pem").to_str().expect("path"),
                "-CAkey",
                self.dir.join("ca.key.pem").to_str().expect("path"),
                "-set_serial",
                &serial.to_string(),
                "-days",
                "30",
                "-extfile",
                ext_path.to_str().expect("path"),
                "-out",
                cert_path.to_str().expect("path"),
            ]);
            let now = Utc::now();
            ManagedCertificate {
                id: uuid::Uuid::now_v7(),
                subject: subject.to_string(),
                cert_pem: std::fs::read_to_string(&cert_path).expect("leaf cert"),
                key_pem: Zeroizing::new(std::fs::read_to_string(&key_path).expect("leaf key")),
                ca_cert_pem: self.ca_cert_pem.clone(),
                issued_at: now - TimeDelta::minutes(1),
                expires_at: now + TimeDelta::days(30),
                updated_at: now,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_pki::TestCa;
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn extract_serial(cert_pem: &str) -> Vec<u8> {
        let (_, pem_obj) = x509_parser::pem::parse_x509_pem(cert_pem.as_bytes()).expect("pem");
        let (_, x509) = x509_parser::parse_x509_certificate(&pem_obj.contents).expect("x509");
        normalize_serial_bytes(x509.raw_serial())
    }

    fn peer_leaf_serial(certs: Option<&[CertificateDer<'static>]>) -> Vec<u8> {
        let leaf = &certs.expect("peers")[0];
        let (_, x509) = x509_parser::parse_x509_certificate(leaf.as_ref()).expect("x509");
        normalize_serial_bytes(x509.raw_serial())
    }

    #[tokio::test]
    async fn client_socket_enables_tcp_nodelay_after_connect() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("local addr");

        let accept = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            stream
        });

        let tcp_stream = TcpStream::connect(addr).await.expect("connect");
        enable_tcp_nodelay(&tcp_stream, addr);

        assert!(
            tcp_stream.nodelay().expect("read client nodelay"),
            "client socket must have TCP_NODELAY enabled"
        );

        let _peer = accept.await.expect("join accept");
    }

    #[test]
    fn verify_peer_rejects_empty_ca_cert_without_degrading() {
        let ca = TestCa::new();
        let mut cert = ca.issue_leaf("mazbri-auth", &["mazbri-auth"]);
        cert.ca_cert_pem.clear();
        let cfg = TlsConfig {
            service_identity: "mazbri-auth".to_string(),
            verify_peer: true,
            ..TlsConfig::default()
        };
        assert!(matches!(
            TlsAcceptor::new(&cert, &cfg),
            Err(TlsError::InvalidCertificate(_))
        ));
        let handle = ManagedCertHandle::from_certificate(cert);
        assert!(matches!(
            TlsAcceptor::from_cert_handle(&handle, &cfg),
            Err(TlsError::InvalidCertificate(_))
        ));
    }

    #[test]
    fn peer_san_acl_enforces_least_privilege_and_blocks_multi_san_spoofing() {
        let ca = TestCa::new();
        let gw_cert = ca.issue_leaf("mazbri-gateway", &["mazbri-gateway"]);
        let ext_cert = ca.issue_leaf("mazbri-cbpro-ext", &["mazbri-cbpro-ext"]);
        let auth_cert = ca.issue_leaf("mazbri-auth", &["mazbri-auth"]);
        let spoof_cert = ca.issue_leaf("mazbri-gateway", &["mazbri-gateway", "mazbri-auth"]);

        let gw_der = parse_pem_certs(&gw_cert.cert_pem, "gw");
        let ext_der = parse_pem_certs(&ext_cert.cert_pem, "ext");
        let auth_der = parse_pem_certs(&auth_cert.cert_pem, "auth");
        let spoof_der = parse_pem_certs(&spoof_cert.cert_pem, "spoof");

        assert!(verify_peer_san_acl("mazbri-billing", Some(&gw_der)).is_ok());
        assert!(verify_peer_san_acl("mazbri-billing", Some(&ext_der)).is_err());
        assert!(verify_peer_san_acl("mazbri-billing", Some(&auth_der)).is_err());
        assert!(verify_peer_san_acl("mazbri-auth", Some(&gw_der)).is_ok());
        assert!(verify_peer_san_acl("mazbri-auth", Some(&ext_der)).is_err());
        assert!(verify_peer_san_acl("mazbri-cbpro", Some(&gw_der)).is_ok());
        assert!(verify_peer_san_acl("mazbri-cbpro", Some(&ext_der)).is_ok());
        assert!(verify_peer_san_acl("mazbri-cbpro", Some(&auth_der)).is_err());
        assert!(verify_peer_san_acl("mazbri-billing", Some(&spoof_der)).is_err());
        assert!(verify_peer_san_acl("mazbri-billing", None).is_err());
    }

    #[tokio::test]
    async fn dynamic_h2_hot_reload_and_revocation_push_work_without_restart() {
        let ca = TestCa::new();
        let srv_v1 = ca.issue_leaf("mazbri-bypass", &["mazbri-bypass"]);
        let cli_v1 = ca.issue_leaf("mazbri-gateway", &["mazbri-gateway"]);
        let srv_v1_serial = extract_serial(&srv_v1.cert_pem);

        let srv_handle = ManagedCertHandle::from_certificate(srv_v1);
        let cli_handle = ManagedCertHandle::from_certificate(cli_v1);
        let srv_cfg = TlsConfig {
            service_identity: "mazbri-bypass".to_string(),
            verify_peer: true,
            ..TlsConfig::default()
        };
        let cli_cfg = TlsConfig {
            service_identity: "mazbri-gateway".to_string(),
            verify_peer: true,
            ..TlsConfig::default()
        };
        let acceptor = TlsAcceptor::new_h2(&srv_handle, &srv_cfg).expect("new_h2 acceptor");
        let connector =
            TlsConnector::new_h2(&cli_handle, &cli_cfg, "mazbri-bypass").expect("new_h2 connector");

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");

        let acc1 = acceptor.clone();
        let srv_task = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("accept 1");
            let mut tls = acc1.accept(tcp).await.expect("tls accept 1");
            assert_eq!(tls.get_ref().1.alpn_protocol(), Some(&b"h2"[..]));
            tls.write_all(b"pong1").await.expect("write");
            (listener, tls)
        });
        let mut cli_tls = connector.connect(addr).await.expect("connect 1");
        assert_eq!(cli_tls.get_ref().1.alpn_protocol(), Some(&b"h2"[..]));
        assert_eq!(
            peer_leaf_serial(cli_tls.get_ref().1.peer_certificates()),
            srv_v1_serial
        );
        let mut buf = [0u8; 5];
        cli_tls.read_exact(&mut buf).await.expect("read");
        assert_eq!(&buf, b"pong1");
        let (listener, _keep_alive_1) = srv_task.await.expect("join 1");

        let srv_v2 = ca.issue_leaf("mazbri-bypass", &["mazbri-bypass"]);
        let cli_v2 = ca.issue_leaf("mazbri-gateway", &["mazbri-gateway"]);
        let srv_v2_serial = extract_serial(&srv_v2.cert_pem);
        let cli_v2_serial = extract_serial(&cli_v2.cert_pem);
        assert_ne!(srv_v1_serial, srv_v2_serial);
        srv_handle.update_certificate(srv_v2).await.expect("swap srv");
        cli_handle.update_certificate(cli_v2).await.expect("swap cli");

        let acc2 = acceptor.clone();
        let srv_task_2 = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("accept 2");
            let _tls = acc2.accept(tcp).await.expect("tls accept 2");
            listener
        });
        let cli_tls_2 = connector.connect(addr).await.expect("connect 2");
        assert_eq!(
            peer_leaf_serial(cli_tls_2.get_ref().1.peer_certificates()),
            srv_v2_serial
        );
        let listener = srv_task_2.await.expect("join 2");

        srv_handle.set_revoked_serials(HashSet::from([cli_v2_serial]));
        let acc3 = acceptor.clone();
        let srv_task_3 = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("accept 3");
            acc3.accept(tcp).await
        });
        let cli_res_3 = connector.connect(addr).await;
        let srv_res_3 = srv_task_3.await.expect("join 3");
        assert!(srv_res_3.is_err() || cli_res_3.is_err());
    }
}
