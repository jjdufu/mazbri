use crate::transport::{enable_tcp_nodelay, verify_peer_san_acl};
use crate::{TlsAcceptor, TlsConnector, TlsError, TlsResult};
use futures::prelude::*;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{Semaphore, mpsc};
use tokio_rustls::{client::TlsStream as ClientTlsStream, server::TlsStream as ServerTlsStream};
use tracing::{debug, info, warn};

const MAX_CONCURRENT_HANDSHAKES: usize = 256;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPTED_CHANNEL_CAPACITY: usize = 256;

pub struct TlsTarpcListener {
    listener: tokio::net::TcpListener,
    acceptor: TlsAcceptor,
}

impl TlsTarpcListener {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn bind(addr: SocketAddr, acceptor: TlsAcceptor) -> TlsResult<Self> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        info!(addr = %addr, "TLS tarpc listener bound");
        Ok(Self { listener, acceptor })
    }

    pub fn listen<Item, SinkItem, Codec, F>(
        self,
        codec_factory: F,
    ) -> impl Stream<
        Item = Result<
            tarpc::serde_transport::Transport<ServerTlsStream<TcpStream>, Item, SinkItem, Codec>,
            TlsError,
        >,
    >
    where
        Item: for<'de> serde::Deserialize<'de>,
        SinkItem: serde::Serialize,
        Codec: tarpc::tokio_serde::Serializer<SinkItem> + tarpc::tokio_serde::Deserializer<Item>,
        F: Fn() -> Codec,
    {
        debug!("Starting bounded concurrent TLS listener stream");
        let (tx, mut rx) = mpsc::channel::<ServerTlsStream<TcpStream>>(ACCEPTED_CHANNEL_CAPACITY);
        let listener = self.listener;
        let acceptor = self.acceptor;
        let handshake_sem = Arc::new(Semaphore::new(MAX_CONCURRENT_HANDSHAKES));

        tokio::spawn(async move {
            loop {
                let permit = tokio::select! {
                    () = tx.closed() => {
                        debug!("TLS tarpc listener stream dropped; stopping accept loop");
                        break;
                    }
                    res = Arc::clone(&handshake_sem).acquire_owned() => {
                        match res {
                            Ok(permit) => permit,
                            Err(_) => break,
                        }
                    }
                };

                let accepted = tokio::select! {
                    () = tx.closed() => {
                        debug!("TLS tarpc listener stream dropped while waiting for TCP accept");
                        break;
                    }
                    res = listener.accept() => res,
                };

                let (tcp_stream, peer_addr) = match accepted {
                    Ok(conn) => conn,
                    Err(e) => {
                        warn!(error = %e, "Failed to accept TCP connection");
                        continue;
                    }
                };

                enable_tcp_nodelay(&tcp_stream, peer_addr);
                let acceptor_clone = acceptor.clone();
                let tx_clone = tx.clone();

                tokio::spawn(async move {
                    let _permit = permit;
                    let handshake =
                        tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor_clone.accept(tcp_stream))
                            .await;
                    let tls_stream = match handshake {
                        Ok(Ok(stream)) => stream,
                        Ok(Err(e)) => {
                            warn!(peer = %peer_addr, error = %e, "TLS handshake failed");
                            return;
                        }
                        Err(_) => {
                            warn!(
                                peer = %peer_addr,
                                timeout_secs = HANDSHAKE_TIMEOUT.as_secs(),
                                "TLS handshake timed out"
                            );
                            return;
                        }
                    };

                    if acceptor_clone.verify_peer() {
                        let peer_certs = tls_stream.get_ref().1.peer_certificates();
                        if let Err(e) =
                            verify_peer_san_acl(acceptor_clone.service_identity(), peer_certs)
                        {
                            warn!(
                                peer = %peer_addr,
                                service_identity = %acceptor_clone.service_identity(),
                                error = %e,
                                "Rejected mTLS tarpc connection failing peer SAN ACL"
                            );
                            return;
                        }
                    }

                    debug!(peer = %peer_addr, "TLS tarpc listener completed handshake and SAN ACL");
                    let _ = tx_clone.send(tls_stream).await;
                });
            }
        });

        async_stream::stream! {
            while let Some(tls_stream) = rx.recv().await {
                let transport = tarpc::serde_transport::Transport::from((tls_stream, codec_factory()));
                yield Ok(transport);
            }
        }
    }
}

pub struct TlsTarpcConnector;

impl TlsTarpcConnector {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn connect<Item, SinkItem, Codec>(
        addr: SocketAddr,
        connector: TlsConnector,
        server_name: Option<&str>,
        codec: Codec,
    ) -> TlsResult<
        tarpc::serde_transport::Transport<ClientTlsStream<TcpStream>, Item, SinkItem, Codec>,
    >
    where
        Item: for<'de> serde::Deserialize<'de>,
        SinkItem: serde::Serialize,
        Codec: tarpc::tokio_serde::Serializer<SinkItem> + tarpc::tokio_serde::Deserializer<Item>,
    {
        debug!(addr = %addr, "Connecting TLS transport for tarpc");
        let tls_stream = connector
            .connect_with_server_name(addr, server_name)
            .await?;
        info!(addr = %addr, "TLS tarpc transport connected");
        Ok(tarpc::serde_transport::Transport::from((tls_stream, codec)))
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn tls_listen<Item, SinkItem, Codec, F>(
    addr: SocketAddr,
    acceptor: TlsAcceptor,
    codec_factory: F,
) -> TlsResult<
    impl Stream<
        Item = Result<
            tarpc::serde_transport::Transport<ServerTlsStream<TcpStream>, Item, SinkItem, Codec>,
            TlsError,
        >,
    >,
>
where
    Item: for<'de> serde::Deserialize<'de>,
    SinkItem: serde::Serialize,
    Codec: tarpc::tokio_serde::Serializer<SinkItem> + tarpc::tokio_serde::Deserializer<Item>,
    F: Fn() -> Codec,
{
    let listener = TlsTarpcListener::bind(addr, acceptor).await?;
    Ok(listener.listen(codec_factory))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn tls_connect<Item, SinkItem, Codec>(
    addr: SocketAddr,
    connector: TlsConnector,
    codec: Codec,
) -> TlsResult<tarpc::serde_transport::Transport<ClientTlsStream<TcpStream>, Item, SinkItem, Codec>>
where
    Item: for<'de> serde::Deserialize<'de>,
    SinkItem: serde::Serialize,
    Codec: tarpc::tokio_serde::Serializer<SinkItem> + tarpc::tokio_serde::Deserializer<Item>,
{
    TlsTarpcConnector::connect(addr, connector, None, codec).await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn tls_connect_with_server_name<Item, SinkItem, Codec>(
    addr: SocketAddr,
    connector: TlsConnector,
    server_name: Option<&str>,
    codec: Codec,
) -> TlsResult<tarpc::serde_transport::Transport<ClientTlsStream<TcpStream>, Item, SinkItem, Codec>>
where
    Item: for<'de> serde::Deserialize<'de>,
    SinkItem: serde::Serialize,
    Codec: tarpc::tokio_serde::Serializer<SinkItem> + tarpc::tokio_serde::Deserializer<Item>,
{
    TlsTarpcConnector::connect(addr, connector, server_name, codec).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::test_pki::TestCa;
    use crate::{ManagedCertHandle, TlsConfig};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn server_socket_enables_tcp_nodelay_after_accept() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("local addr");

        let client = tokio::spawn(async move {
            TcpStream::connect(addr).await.expect("client connect")
        });

        let (tcp_stream, peer_addr) = listener.accept().await.expect("accept");
        crate::transport::enable_tcp_nodelay(&tcp_stream, peer_addr);

        assert!(
            tcp_stream.nodelay().expect("read server nodelay"),
            "server socket must have TCP_NODELAY enabled after accept"
        );

        let _client = client.await.expect("join client");
    }

    #[tokio::test]
    async fn stalled_tcp_connection_does_not_block_valid_peer_and_unauthorized_san_is_dropped() {
        let ca = TestCa::new();
        let billing_cert = ca.issue_leaf("mazbri-billing", &["mazbri-billing"]);
        let gateway_cert = ca.issue_leaf("mazbri-gateway", &["mazbri-gateway"]);
        let unauthorized_cert = ca.issue_leaf("mazbri-cbpro-ext", &["mazbri-cbpro-ext"]);

        let srv_handle = ManagedCertHandle::from_certificate(billing_cert);
        let gw_handle = ManagedCertHandle::from_certificate(gateway_cert);
        let unauth_handle = ManagedCertHandle::from_certificate(unauthorized_cert);

        let srv_cfg = TlsConfig {
            service_identity: "mazbri-billing".to_string(),
            verify_peer: true,
            ..TlsConfig::default()
        };
        let acceptor = TlsAcceptor::from_cert_handle(&srv_handle, &srv_cfg).expect("acceptor");
        let listener = TlsTarpcListener::bind("127.0.0.1:0".parse().expect("addr"), acceptor)
            .await
            .expect("bind");
        let addr = listener.listener.local_addr().expect("local addr");
        let mut incoming = Box::pin(listener.listen(tarpc::tokio_serde::formats::Bincode::<
            u64,
            u64,
        >::default));

        let mut slow_clients = Vec::new();
        for _ in 0..4 {
            slow_clients.push(TcpStream::connect(addr).await.expect("slow tcp"));
        }

        let unauth_connector = TlsConnector::from_cert_handle_with_domain(
            &unauth_handle,
            &TlsConfig {
                service_identity: "mazbri-cbpro-ext".to_string(),
                verify_peer: true,
                ..TlsConfig::default()
            },
            "mazbri-billing",
        )
        .expect("unauth connector");
        let _unauth_conn = TlsTarpcConnector::connect::<u64, u64, _>(
            addr,
            unauth_connector,
            Some("mazbri-billing"),
            tarpc::tokio_serde::formats::Bincode::default(),
        )
        .await
        .expect("tls handshake completes before server drops stream on SAN ACL");

        let gw_connector = TlsConnector::from_cert_handle_with_domain(
            &gw_handle,
            &TlsConfig {
                service_identity: "mazbri-gateway".to_string(),
                verify_peer: true,
                ..TlsConfig::default()
            },
            "mazbri-billing",
        )
        .expect("gw connector");
        let gw_task = tokio::spawn(async move {
            TlsTarpcConnector::connect::<u64, u64, _>(
                addr,
                gw_connector,
                Some("mazbri-billing"),
                tarpc::tokio_serde::formats::Bincode::default(),
            )
            .await
            .expect("gw connect")
        });

        let accepted = tokio::time::timeout(Duration::from_millis(800), incoming.next())
            .await
            .expect("must not be blocked by stalled handshakes")
            .expect("stream item");
        assert!(accepted.is_ok());
        let _gw_transport = gw_task.await.expect("gw join");
        drop(slow_clients);
    }
}
