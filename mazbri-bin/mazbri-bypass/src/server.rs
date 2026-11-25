use crate::service::MazbriBypassServiceImpl;
use anyhow::Result;
use futures::{FutureExt, stream};
use mazbri_core::panic_policy::{PanicError, recoverable};
use mazbri_tls::{
    ManagedCertHandle, ServerTlsStream, TlsAcceptor, TlsConfig, verify_peer_san_acl,
};
use mazbri_upstream_contracts::shared::{
    HealthRequest, HealthResponse, InvokeRequest, InvokeResponse,
};
use mazbri_upstream_contracts::{BYPASS_EXECUTE_PATH, BYPASS_HEALTH_PATH, BYPASS_SERVICE_NAME};
use std::convert::Infallible;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, mpsc};
use tonic::body::Body;
use tonic::server::{Grpc, NamedService, UnaryService};
use tonic::transport::Server;
use tonic::transport::server::{Connected, TcpConnectInfo};
use tonic::{Code, Request, Response, Status};
use tonic_prost::ProstCodec;
use tracing::{error, info, warn};

type BoxFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'static>>;

/// Maximum gRPC message size between gateway and bypass (16 MiB).
pub const MAX_BYPASS_GRPC_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

pub trait BypassRpcHandler: Send + Sync + 'static {
    fn execute(
        &self,
        request: Request<InvokeRequest>,
    ) -> impl Future<Output = Response<InvokeResponse>> + Send;

    fn health(&self) -> Response<HealthResponse>;
}

impl BypassRpcHandler for MazbriBypassServiceImpl {
    fn execute(
        &self,
        request: Request<InvokeRequest>,
    ) -> impl Future<Output = Response<InvokeResponse>> + Send {
        MazbriBypassServiceImpl::execute(self, request)
    }

    fn health(&self) -> Response<HealthResponse> {
        MazbriBypassServiceImpl::health(self)
    }
}

/// Hand-written unary gRPC server for the bypass contract (messages come from
/// `mazbri-upstream-contracts`, routing is by method path).
pub struct BypassGrpcServer<S = MazbriBypassServiceImpl> {
    inner: Arc<S>,
}

impl<S> Clone for BypassGrpcServer<S> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl BypassGrpcServer<MazbriBypassServiceImpl> {
    #[must_use]
    pub fn new(service: MazbriBypassServiceImpl) -> Self {
        Self {
            inner: Arc::new(service),
        }
    }
}

impl<S: BypassRpcHandler> BypassGrpcServer<S> {
    #[cfg(test)]
    fn with_handler(handler: S) -> Self {
        Self {
            inner: Arc::new(handler),
        }
    }
}

impl<S: BypassRpcHandler> NamedService for BypassGrpcServer<S> {
    const NAME: &'static str = BYPASS_SERVICE_NAME;
}

struct ExecuteMethod<S>(Arc<S>);

impl<S: BypassRpcHandler> UnaryService<InvokeRequest> for ExecuteMethod<S> {
    type Response = InvokeResponse;
    type Future = BoxFuture<Response<InvokeResponse>, Status>;

    fn call(&mut self, request: Request<InvokeRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        let request_id = request.get_ref().request_id.clone();
        Box::pin(async move {
            let fut = async move { Box::pin(inner.execute(request)).await };
            match AssertUnwindSafe(recoverable(fut)).catch_unwind().await {
                Ok(response) => Ok(response),
                Err(payload) => {
                    let err = PanicError::from_payload(payload.as_ref());
                    error!(
                        rpc = "Execute",
                        request_id = %request_id,
                        panic = %err.message,
                        "Bypass gRPC Execute panicked; returning Status::internal"
                    );
                    Err(Status::internal("internal bypass error"))
                }
            }
        })
    }
}

struct HealthMethod<S>(Arc<S>);

impl<S: BypassRpcHandler> UnaryService<HealthRequest> for HealthMethod<S> {
    type Response = HealthResponse;
    type Future = BoxFuture<Response<HealthResponse>, Status>;

    fn call(&mut self, _request: Request<HealthRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move {
            let fut = async move { inner.health() };
            match AssertUnwindSafe(recoverable(fut)).catch_unwind().await {
                Ok(response) => Ok(response),
                Err(payload) => {
                    let err = PanicError::from_payload(payload.as_ref());
                    error!(
                        rpc = "Health",
                        panic = %err.message,
                        "Bypass gRPC Health panicked; returning Status::internal"
                    );
                    Err(Status::internal("internal bypass error"))
                }
            }
        })
    }
}

impl<S: BypassRpcHandler> tower::Service<http::Request<Body>> for BypassGrpcServer<S> {
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<Body>) -> Self::Future {
        let inner = Arc::clone(&self.inner);
        match req.uri().path() {
            BYPASS_EXECUTE_PATH => Box::pin(async move {
                let fut = async move {
                    let mut grpc = Grpc::new(ProstCodec::<InvokeResponse, InvokeRequest>::default())
                        .max_decoding_message_size(MAX_BYPASS_GRPC_MESSAGE_BYTES)
                        .max_encoding_message_size(MAX_BYPASS_GRPC_MESSAGE_BYTES);
                    grpc.unary(ExecuteMethod(inner), req).await
                };
                match AssertUnwindSafe(recoverable(fut)).catch_unwind().await {
                    Ok(resp) => Ok(resp),
                    Err(payload) => {
                        let err = PanicError::from_payload(payload.as_ref());
                        error!(
                            rpc = "Execute",
                            panic = %err.message,
                            "Bypass gRPC Execute framing panicked; returning Status::internal"
                        );
                        Ok(Status::internal("internal bypass error").into_http())
                    }
                }
            }),
            BYPASS_HEALTH_PATH => Box::pin(async move {
                let fut = async move {
                    let mut grpc = Grpc::new(ProstCodec::<HealthResponse, HealthRequest>::default())
                        .max_decoding_message_size(MAX_BYPASS_GRPC_MESSAGE_BYTES)
                        .max_encoding_message_size(MAX_BYPASS_GRPC_MESSAGE_BYTES);
                    grpc.unary(HealthMethod(inner), req).await
                };
                match AssertUnwindSafe(recoverable(fut)).catch_unwind().await {
                    Ok(resp) => Ok(resp),
                    Err(payload) => {
                        let err = PanicError::from_payload(payload.as_ref());
                        error!(
                            rpc = "Health",
                            panic = %err.message,
                            "Bypass gRPC Health framing panicked; returning Status::internal"
                        );
                        Ok(Status::internal("internal bypass error").into_http())
                    }
                }
            }),
            _ => Box::pin(async move { Ok(Status::new(Code::Unimplemented, "").into_http()) }),
        }
    }
}

struct BypassMtlsStream(ServerTlsStream<TcpStream>);

impl Connected for BypassMtlsStream {
    type ConnectInfo = TcpConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        self.0.get_ref().0.connect_info()
    }
}

impl AsyncRead for BypassMtlsStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for BypassMtlsStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

const BYPASS_TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn serve(
    addr: SocketAddr,
    service: MazbriBypassServiceImpl,
    cert_handle: ManagedCertHandle,
    tls_config: TlsConfig,
    max_connections: u32,
) -> Result<()> {
    let acceptor = TlsAcceptor::new_h2(&cert_handle, &tls_config)
        .map_err(|e| anyhow::anyhow!("failed to configure dynamic H2 mTLS acceptor: {e}"))?;
    let tcp_listener = TcpListener::bind(addr).await?;
    let max_handshakes = (max_connections as usize).clamp(32, 1024);
    let semaphore = Arc::new(Semaphore::new(max_handshakes));
    let (tx, rx) = mpsc::channel::<io::Result<BypassMtlsStream>>(max_handshakes);

    tokio::spawn(async move {
        loop {
            let (tcp_stream, peer_addr) = match tcp_listener.accept().await {
                Ok(pair) => pair,
                Err(err) => {
                    warn!(error = %err, "mazbri-bypass TCP accept error");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            };
            let _ = tcp_stream.set_nodelay(true);
            let Ok(permit) = Arc::clone(&semaphore).try_acquire_owned() else {
                warn!(peer = %peer_addr, "mazbri-bypass TLS handshake concurrency limit reached; dropping connection");
                continue;
            };
            let acceptor = acceptor.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let _permit = permit;
                match tokio::time::timeout(BYPASS_TLS_HANDSHAKE_TIMEOUT, acceptor.accept(tcp_stream))
                    .await
                {
                    Ok(Ok(tls_stream)) => {
                        let peer_certs = tls_stream.get_ref().1.peer_certificates();
                        if let Err(acl_err) = verify_peer_san_acl("mazbri-bypass", peer_certs) {
                            warn!(peer = %peer_addr, error = %acl_err, "mazbri-bypass rejected mTLS peer by SAN ACL");
                            return;
                        }
                        let _ = tx.send(Ok(BypassMtlsStream(tls_stream))).await;
                    }
                    Ok(Err(err)) => {
                        warn!(peer = %peer_addr, error = %err, "mazbri-bypass mTLS handshake failed");
                    }
                    Err(_) => {
                        warn!(peer = %peer_addr, "mazbri-bypass mTLS handshake timed out after 5s");
                    }
                }
            });
        }
    });

    let incoming = stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    });

    info!(
        "mazbri-bypass Egress HTTP/2 server listening on {} (dynamic mTLS ALPN=h2 + Peer SAN ACL, max connections: {}, contract=BYPASS-HTTP-PROXY)",
        addr, max_connections
    );
    Server::builder()
        .http2_adaptive_window(Some(true))
        .initial_connection_window_size(Some(4 * 1024 * 1024))
        .initial_stream_window_size(Some(1024 * 1024))
        .concurrency_limit_per_connection(max_connections as usize)
        .add_service(BypassGrpcServer::new(service))
        .serve_with_incoming(incoming)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_core::panic_policy::PanicDisposition;
    use std::sync::Mutex;

    struct FaultyBypassHandler {
        panic_health: bool,
        observed: Arc<Mutex<Vec<PanicDisposition>>>,
    }

    impl BypassRpcHandler for FaultyBypassHandler {
        async fn execute(&self, request: Request<InvokeRequest>) -> Response<InvokeResponse> {
            tokio::task::yield_now().await;
            let req = request.into_inner();
            if req.request_id == "boom" {
                self.observed
                    .lock()
                    .expect("lock")
                    .push(PanicDisposition::current());
                panic!("boom in execute");
            }
            Response::new(InvokeResponse {
                success: true,
                payload_json: format!("{{\"echo\":\"{}\"}}", req.request_id),
                ..InvokeResponse::default()
            })
        }

        fn health(&self) -> Response<HealthResponse> {
            if self.panic_health {
                self.observed
                    .lock()
                    .expect("lock")
                    .push(PanicDisposition::current());
                panic!("boom in health");
            }
            Response::new(HealthResponse {
                status: "ok".to_string(),
                service: "mazbri-bypass".to_string(),
                timestamp: 1,
            })
        }
    }

    #[tokio::test]
    async fn execute_and_health_panics_return_internal_status_without_killing_service() {
        let observed = Arc::new(Mutex::new(Vec::new()));
        let server = BypassGrpcServer::with_handler(FaultyBypassHandler {
            panic_health: false,
            observed: Arc::clone(&observed),
        });
        let mut client = tonic::client::Grpc::new(server);
        client.ready().await.expect("ready");

        let exec_path = http::uri::PathAndQuery::from_static(BYPASS_EXECUTE_PATH);
        let err = client
            .unary(
                Request::new(InvokeRequest {
                    request_id: "boom".to_string(),
                    ..InvokeRequest::default()
                }),
                exec_path.clone(),
                ProstCodec::<InvokeRequest, InvokeResponse>::default(),
            )
            .await
            .expect_err("panicking Execute must return Status::internal");
        assert_eq!(err.code(), Code::Internal);
        assert_eq!(err.message(), "internal bypass error");

        // Same service instance handles subsequent healthy Execute and Health calls.
        client.ready().await.expect("ready");
        let ok_resp = client
            .unary(
                Request::new(InvokeRequest {
                    request_id: "ok-1".to_string(),
                    ..InvokeRequest::default()
                }),
                exec_path,
                ProstCodec::<InvokeRequest, InvokeResponse>::default(),
            )
            .await
            .expect("healthy Execute must succeed");
        assert!(ok_resp.into_inner().success);

        client.ready().await.expect("ready");
        let health_path = http::uri::PathAndQuery::from_static(BYPASS_HEALTH_PATH);
        let health_ok = client
            .unary(
                Request::new(HealthRequest {}),
                health_path.clone(),
                ProstCodec::<HealthRequest, HealthResponse>::default(),
            )
            .await
            .expect("healthy Health must succeed");
        assert_eq!(health_ok.into_inner().status, "ok");

        // Verify Health panic isolation as well.
        let health_panic_server = BypassGrpcServer::with_handler(FaultyBypassHandler {
            panic_health: true,
            observed: Arc::clone(&observed),
        });
        let mut panic_client = tonic::client::Grpc::new(health_panic_server);
        panic_client.ready().await.expect("ready");
        let health_err = panic_client
            .unary(
                Request::new(HealthRequest {}),
                health_path,
                ProstCodec::<HealthRequest, HealthResponse>::default(),
            )
            .await
            .expect_err("panicking Health must return Status::internal");
        assert_eq!(health_err.code(), Code::Internal);
        assert_eq!(health_err.message(), "internal bypass error");

        assert_eq!(
            *observed.lock().expect("lock"),
            vec![PanicDisposition::Recovered, PanicDisposition::Recovered]
        );
    }
}
