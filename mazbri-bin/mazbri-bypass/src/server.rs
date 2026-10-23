use crate::service::MazbriBypassServiceImpl;
use anyhow::Result;
use mazbri_tls::ManagedCertificate;
use mazbri_upstream_contracts::shared::{
    HealthRequest, HealthResponse, InvokeRequest, InvokeResponse,
};
use mazbri_upstream_contracts::{BYPASS_EXECUTE_PATH, BYPASS_HEALTH_PATH, BYPASS_SERVICE_NAME};
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tonic::body::Body;
use tonic::server::{Grpc, NamedService, UnaryService};
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tonic::{Code, Request, Response, Status};
use tonic_prost::ProstCodec;
use tracing::info;

type BoxFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'static>>;

/// Hand-written unary gRPC server for the bypass contract (messages come from
/// `mazbri-upstream-contracts`, routing is by method path).
#[derive(Clone)]
pub struct BypassGrpcServer {
    inner: Arc<MazbriBypassServiceImpl>,
}

impl BypassGrpcServer {
    #[must_use]
    pub fn new(service: MazbriBypassServiceImpl) -> Self {
        Self {
            inner: Arc::new(service),
        }
    }
}

impl NamedService for BypassGrpcServer {
    const NAME: &'static str = BYPASS_SERVICE_NAME;
}

struct ExecuteMethod(Arc<MazbriBypassServiceImpl>);

impl UnaryService<InvokeRequest> for ExecuteMethod {
    type Response = InvokeResponse;
    type Future = BoxFuture<Response<InvokeResponse>, Status>;

    fn call(&mut self, request: Request<InvokeRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { Ok(inner.execute(request).await) })
    }
}

struct HealthMethod(Arc<MazbriBypassServiceImpl>);

impl UnaryService<HealthRequest> for HealthMethod {
    type Response = HealthResponse;
    type Future = BoxFuture<Response<HealthResponse>, Status>;

    fn call(&mut self, _request: Request<HealthRequest>) -> Self::Future {
        let response = self.0.health();
        Box::pin(async move { Ok(response) })
    }
}

impl tower::Service<http::Request<Body>> for BypassGrpcServer {
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
                let mut grpc = Grpc::new(ProstCodec::<InvokeResponse, InvokeRequest>::default());
                Ok(grpc.unary(ExecuteMethod(inner), req).await)
            }),
            BYPASS_HEALTH_PATH => Box::pin(async move {
                let mut grpc = Grpc::new(ProstCodec::<HealthResponse, HealthRequest>::default());
                Ok(grpc.unary(HealthMethod(inner), req).await)
            }),
            _ => Box::pin(async move { Ok(Status::new(Code::Unimplemented, "").into_http()) }),
        }
    }
}

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
        .add_service(BypassGrpcServer::new(service))
        .serve(addr)
        .await?;
    Ok(())
}
