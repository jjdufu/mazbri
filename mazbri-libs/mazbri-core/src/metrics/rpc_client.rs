//! tarpc client RED metrics via a Stub wrapper.
use super::{MetricsBuilder, register_collector_once};
use crate::REGISTRY;
use prometheus::{CounterVec, HistogramVec};
use std::sync::LazyLock;
use std::time::Instant;
use tarpc::client::stub::Stub;
use tarpc::client::{Channel, NewClient, RpcError};
use tarpc::{ClientMessage, RequestName, Response, Transport};

pub static RPC_CLIENT_REQUESTS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "mazbri_rpc_client_requests_total",
        "tarpc client requests by caller, callee, method and outcome code",
        &["caller", "callee", "method", "code"],
    )
});

pub static RPC_CLIENT_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "mazbri_rpc_client_duration_seconds",
        "tarpc client request duration in seconds",
        vec![
            0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ],
        &["caller", "callee"],
    )
});

/// Register RPC client collectors (idempotent).
pub fn register_rpc_client_metrics() {
    if let Err(error) =
        register_collector_once(&REGISTRY, Box::new(RPC_CLIENT_REQUESTS_TOTAL.clone()))
    {
        tracing::warn!(%error, "mazbri_rpc_client_requests_total registration failed");
    }
    if let Err(error) =
        register_collector_once(&REGISTRY, Box::new(RPC_CLIENT_DURATION_SECONDS.clone()))
    {
        tracing::warn!(%error, "mazbri_rpc_client_duration_seconds registration failed");
    }
}

fn classify_rpc_code(result: &Result<impl Sized, RpcError>) -> &'static str {
    match result {
        Ok(_) => "ok",
        Err(RpcError::DeadlineExceeded) => "deadline",
        Err(RpcError::Server(_)) => "app_error",
        Err(RpcError::Shutdown | RpcError::Send(_) | RpcError::Channel(_)) => "transport",
    }
}

/// Stub wrapper that records `mazbri_rpc_client_*` on every call.
#[derive(Clone, Debug)]
pub struct RpcMetricsStub<S> {
    inner: S,
    /// Prometheus `caller` label (canonical service name).
    from_svc: &'static str,
    /// Prometheus `callee` label (canonical service name).
    to_svc: &'static str,
}

impl<S> RpcMetricsStub<S> {
    #[must_use]
    pub fn new(inner: S, from_svc: &'static str, to_svc: &'static str) -> Self {
        Self {
            inner,
            from_svc,
            to_svc,
        }
    }
}

impl<S> Stub for RpcMetricsStub<S>
where
    S: Stub,
{
    type Req = S::Req;
    type Resp = S::Resp;

    async fn call(
        &self,
        ctx: tarpc::context::Context,
        request: Self::Req,
    ) -> Result<Self::Resp, RpcError> {
        let method = request.name().to_string();
        let started = Instant::now();
        let result = self.inner.call(ctx, request).await;
        let code = classify_rpc_code(&result);
        RPC_CLIENT_REQUESTS_TOTAL
            .with_label_values(&[self.from_svc, self.to_svc, method.as_str(), code])
            .inc();
        RPC_CLIENT_DURATION_SECONDS
            .with_label_values(&[self.from_svc, self.to_svc])
            .observe(started.elapsed().as_secs_f64());
        result
    }
}

/// Create a Channel, spawn its dispatch, wrap with [`RpcMetricsStub`], then convert to client `C`.
///
/// Replaces `SomeServiceClient::new(config, transport).spawn()` at call sites.
pub fn spawn_rpc_client<C, Req, Resp, T>(
    config: tarpc::client::Config,
    transport: T,
    from_svc: &'static str,
    to_svc: &'static str,
) -> C
where
    Req: RequestName + Send + Sync + 'static,
    Resp: Send + 'static,
    T: Transport<ClientMessage<Req>, Response<Resp>> + Send + 'static,
    C: From<RpcMetricsStub<Channel<Req, Resp>>>,
{
    let NewClient { client, dispatch } = tarpc::client::new(config, transport);
    tokio::spawn(async move {
        if let Err(error) = dispatch.await {
            tracing::debug!(%error, "RPC client dispatch ended");
        }
    });
    C::from(RpcMetricsStub::new(client, from_svc, to_svc))
}

/// Channel type behind an instrumented service client.
pub type RpcMetricsChannel<Req, Resp> = RpcMetricsStub<Channel<Req, Resp>>;
