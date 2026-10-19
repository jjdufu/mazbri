use crate::{TlsAcceptor, TlsConnector, TlsError, TlsResult};
use futures::prelude::*;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio_rustls::{client::TlsStream as ClientTlsStream, server::TlsStream as ServerTlsStream};
use tracing::{debug, info, warn};
pub struct TlsTarpcListener {
    listener: tokio::net::TcpListener,
    acceptor: TlsAcceptor,
}
impl TlsTarpcListener {
    pub async fn bind(addr: SocketAddr, acceptor: TlsAcceptor) -> TlsResult<Self> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        info!(addr = % addr, "TLS tarpc listener bound");
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
        debug!("Starting TLS listener stream");
        async_stream::stream! {
            loop { let (tcp_stream, peer_addr) = match self.listener.accept(). await {
            Ok(conn) => conn, Err(e) => { warn!("Failed to accept connection: {}", e);
            continue; } }; info!(peer = % peer_addr,
            "TLS tarpc listener accepted TCP connection"); let tls_stream = match self
            .acceptor.accept(tcp_stream). await { Ok(stream) => stream, Err(e) => {
            warn!(peer = % peer_addr, error = % e, "TLS handshake failed"); continue; }
            }; info!(peer = % peer_addr, "TLS tarpc listener completed handshake");
            debug!(peer = % peer_addr, "Constructing tarpc serde transport"); let
            transport = tarpc::serde_transport::Transport::from((tls_stream,
            codec_factory())); info!(peer = % peer_addr,
            "TLS tarpc listener yielding transport"); yield Ok(transport); }
        }
    }
}
pub struct TlsTarpcConnector;
impl TlsTarpcConnector {
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
        debug!(addr = % addr, "Connecting TLS transport for tarpc");
        let tls_stream = connector
            .connect_with_server_name(addr, server_name)
            .await?;
        info!(addr = % addr, "TLS tarpc transport connected");
        Ok(tarpc::serde_transport::Transport::from((tls_stream, codec)))
    }
}
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
