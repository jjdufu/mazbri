use crate::config::ServiceEndpoint;
use mazbri_core::{CoreError, IsTransient};
use mazbri_rpc::{
    CBProExtServiceClient, ObservationRequest, ObservationResponse, RpcError,
    RuntimeDecisionRequest, RuntimeDecisionResponse,
};
use mazbri_tls::ManagedCertificate;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

struct CachedClient {
    client: CBProExtServiceClient,
    created_at: Instant,
}

/// Gateway-side CBPro-Ext RPC client with TTL + reconnect-on-transient-failure.
#[derive(Clone)]
pub struct CBProExtClient {
    endpoint: ServiceEndpoint,
    client: Arc<RwLock<Option<CachedClient>>>,
    tls_connector: mazbri_tls::TlsConnector,
    client_ttl: Duration,
}

impl CBProExtClient {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn new(
        endpoint: ServiceEndpoint,
        tls_ca_cert: &str,
        client_cert: &ManagedCertificate,
        tls_config: &mazbri_tls::TlsConfig,
        client_ttl_secs: u64,
    ) -> Result<Self, CoreError> {
        let tls_connector =
            mazbri_tls::TlsConnector::new(tls_ca_cert, Some(client_cert), tls_config)
                .map_err(|error| CoreError::Tls(error.to_string()))?;
        Ok(Self {
            endpoint,
            client: Arc::new(RwLock::new(None)),
            tls_connector,
            client_ttl: Duration::from_secs(client_ttl_secs.max(1)),
        })
    }

    async fn invalidate_client(&self) {
        let mut guard = self.client.write().await;
        if guard.take().is_some() {
            debug!(
                url = %self.endpoint.url(),
                "Invalidated cached CBPro-Ext tarpc client"
            );
        }
    }

    async fn get_client(&self) -> Result<CBProExtServiceClient, CoreError> {
        {
            let guard = self.client.read().await;
            if let Some(cached) = guard.as_ref() {
                if cached.created_at.elapsed() < self.client_ttl {
                    debug!(url = %self.endpoint.url(), "Using cached CBPro-Ext client");
                    return Ok(cached.client.clone());
                }
            }
        }
        {
            let mut guard = self.client.write().await;
            if let Some(cached) = guard.as_ref() {
                if cached.created_at.elapsed() < self.client_ttl {
                    return Ok(cached.client.clone());
                }
            }
            *guard = None;
        }

        debug!(url = %self.endpoint.url(), "Creating new CBPro-Ext client (mTLS)");
        let target = mazbri_tls::resolve_connect_target(self.endpoint.url(), None)
            .await
            .map_err(|error| CoreError::Config(error.to_string()))?;
        let transport = mazbri_tls::tls_connect_with_server_name(
            target.socket_addr,
            self.tls_connector.clone(),
            target.server_name.as_deref(),
            tarpc::tokio_serde::formats::Bincode::default(),
        )
        .await
        .map_err(|error| CoreError::Tls(error.to_string()))?;
        let new_client =
            CBProExtServiceClient::new(tarpc::client::Config::default(), transport).spawn();
        {
            let mut guard = self.client.write().await;
            *guard = Some(CachedClient {
                client: new_client.clone(),
                created_at: Instant::now(),
            });
        }
        info!(
            url = %self.endpoint.url(),
            server_name = ?target.server_name,
            "CBPro-Ext tarpc client connected (mTLS)"
        );
        Ok(new_client)
    }

    async fn call_with_timeout<T, F>(&self, future: F) -> Result<T, CoreError>
    where
        F: std::future::Future<Output = T>,
    {
        let timeout = self
            .endpoint
            .timeout()
            .max(std::time::Duration::from_millis(1));
        tokio::time::timeout(timeout, future).await.map_err(|_| {
            CoreError::Network(format!(
                "cbpro-ext timeout after {} ms",
                timeout.as_millis()
            ))
        })
    }

    /// One reconnect attempt after transient RPC / timeout errors.
    async fn call_rpc_once_with_reconnect<T, F, Fut>(
        &self,
        op: F,
    ) -> Result<Result<T, tarpc::client::RpcError>, CoreError>
    where
        F: Fn(CBProExtServiceClient) -> Fut + Clone,
        Fut: std::future::Future<Output = Result<T, tarpc::client::RpcError>>,
    {
        let client = self.get_client().await?;
        let op_clone = op.clone();
        match self.call_with_timeout(op_clone(client)).await {
            Ok(Ok(v)) => Ok(Ok(v)),
            Ok(Err(e)) if e.is_transient() => {
                warn!(
                    error = %e,
                    "Transient CBPro-Ext RPC error, invalidating client and retrying once"
                );
                self.invalidate_client().await;
                let client = self.get_client().await?;
                self.call_with_timeout(op(client)).await
            }
            Ok(Err(e)) => Ok(Err(e)),
            Err(timeout_err) => {
                // Timeout may leave a half-dead channel; drop and one reconnect try.
                warn!(
                    error = %timeout_err,
                    "CBPro-Ext call timed out, invalidating client and retrying once"
                );
                self.invalidate_client().await;
                let client = self.get_client().await?;
                self.call_with_timeout(op(client)).await
            }
        }
    }

    pub async fn get_runtime_decision(
        &self,
        request: RuntimeDecisionRequest,
    ) -> RuntimeDecisionResponse {
        match self
            .call_rpc_once_with_reconnect(|client| {
                let request = request.clone();
                async move {
                    client
                        .get_runtime_decision(tarpc::context::current(), request)
                        .await
                }
            })
            .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => RuntimeDecisionResponse {
                available: false,
                decision: None,
                error: Some(RpcError::ServiceUnavailable {
                    service: mazbri_core::ServiceName::CbproExt.as_str().into(),
                    message: error.to_string(),
                }),
            },
            Err(error) => RuntimeDecisionResponse {
                available: false,
                decision: None,
                error: Some(RpcError::ServiceUnavailable {
                    service: mazbri_core::ServiceName::CbproExt.as_str().into(),
                    message: error.to_string(),
                }),
            },
        }
    }

    pub async fn record_observation(&self, request: ObservationRequest) -> ObservationResponse {
        match self
            .call_rpc_once_with_reconnect(|client| {
                let request = request.clone();
                async move {
                    client
                        .record_observation(tarpc::context::current(), request)
                        .await
                }
            })
            .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => ObservationResponse {
                success: false,
                error: Some(RpcError::ServiceUnavailable {
                    service: mazbri_core::ServiceName::CbproExt.as_str().into(),
                    message: error.to_string(),
                }),
            },
            Err(error) => ObservationResponse {
                success: false,
                error: Some(RpcError::ServiceUnavailable {
                    service: mazbri_core::ServiceName::CbproExt.as_str().into(),
                    message: error.to_string(),
                }),
            },
        }
    }

    pub async fn record_observations(&self, requests: Vec<ObservationRequest>) {
        for request in requests {
            let _ = self.record_observation(request).await;
        }
    }
}
