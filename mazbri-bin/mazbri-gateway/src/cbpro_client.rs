use crate::config::ServiceEndpoint;
use mazbri_core::{CoreError, DEFAULT_RETRY_CONFIG, IsTransient, RetryConfig};
use mazbri_rpc::{
    CBProServiceClient, FallbackRequest, FallbackResponse, FallbackSource, RpcError, StoreRequest,
    StoreResponse,
};
use mazbri_tls::ManagedCertificate;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Cached tarpc client with creation time for TTL eviction.
struct CachedClient {
    client: CBProServiceClient,
    created_at: Instant,
}

/// Gateway-side `CBPro` RPC client.
///
/// Holds at most one live tarpc channel. On transient transport errors
/// (`Shutdown` / `Send` / `Channel` / deadline) the cached client is dropped and
/// the next attempt reconnects — otherwise a dead channel would stick until
/// process restart.
#[derive(Clone)]
pub struct CBProClient {
    endpoint: ServiceEndpoint,
    client: Arc<RwLock<Option<CachedClient>>>,
    tls_connector: mazbri_tls::TlsConnector,
    client_ttl: Duration,
}

impl CBProClient {
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
                .map_err(|e| CoreError::Tls(e.to_string()))?;
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
                "Invalidated cached CBPro tarpc client"
            );
        }
    }

    async fn get_client(&self) -> Result<CBProServiceClient, CoreError> {
        {
            let guard = self.client.read().await;
            if let Some(cached) = guard.as_ref()
                && cached.created_at.elapsed() < self.client_ttl
            {
                debug!(url = %self.endpoint.url(), "Using cached CBPro client");
                return Ok(cached.client.clone());
            }
        }

        // Drop expired entry under write lock, then connect.
        {
            let mut guard = self.client.write().await;
            if let Some(cached) = guard.as_ref()
                && cached.created_at.elapsed() < self.client_ttl
            {
                return Ok(cached.client.clone());
            }
            *guard = None;
        }

        debug!(url = %self.endpoint.url(), "Creating new CBPro client (mTLS)");
        let target = mazbri_tls::resolve_connect_target(self.endpoint.url())
            .await
            .map_err(|e| CoreError::Config(e.to_string()))?;
        let transport = mazbri_tls::tls_connect_with_server_name(
            target.socket_addr,
            self.tls_connector.clone(),
            target.server_name.as_deref(),
            tarpc::tokio_serde::formats::Bincode::default(),
        )
        .await
        .map_err(|e| CoreError::Tls(e.to_string()))?;
        let new_client =
            CBProServiceClient::new(tarpc::client::Config::default(), transport).spawn();
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
            "CBPro tarpc client connected (mTLS)"
        );
        Ok(new_client)
    }

    /// Run an RPC, reconnecting on transient transport failures.
    ///
    /// Unlike `retry_rpc` alone, each retry after a transient error drops the
    /// cached channel so `get_client` builds a fresh connection.
    async fn call_with_reconnect<T, F, Fut>(
        &self,
        config: &RetryConfig,
        op: F,
    ) -> Result<T, tarpc::client::RpcError>
    where
        F: Fn(CBProServiceClient) -> Fut,
        Fut: std::future::Future<Output = Result<T, tarpc::client::RpcError>>,
    {
        let mut attempt: u32 = 0;
        loop {
            let client = match self.get_client().await {
                Ok(c) => c,
                Err(e) => {
                    // Connection-level failure: treat like transient for a few tries.
                    if attempt < config.max_retries {
                        let cap =
                            config.initial_delay_ms * u64::from(config.multiplier).pow(attempt);
                        let delay_ms = cap; // deterministic small backoff
                        warn!(
                            attempt = attempt + 1,
                            max_retries = config.max_retries,
                            delay_ms,
                            error = %e,
                            "CBPro connect failed, retrying after invalidate"
                        );
                        self.invalidate_client().await;
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(tarpc::client::RpcError::Shutdown);
                }
            };

            match op(client).await {
                Ok(val) => return Ok(val),
                Err(e) if e.is_transient() && attempt < config.max_retries => {
                    let cap = config.initial_delay_ms * u64::from(config.multiplier).pow(attempt);
                    let delay_ms = cap;
                    warn!(
                        attempt = attempt + 1,
                        max_retries = config.max_retries,
                        delay_ms,
                        error = %e,
                        "Transient CBPro RPC error, invalidating client and retrying"
                    );
                    self.invalidate_client().await;
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    attempt += 1;
                }
                Err(e) => {
                    if e.is_transient() {
                        self.invalidate_client().await;
                    }
                    return Err(e);
                }
            }
        }
    }

    pub async fn get_fallback(&self, request: FallbackRequest) -> FallbackResponse {
        debug!(
            product_id = %request.product_id,
            tenant_id = %request.tenant_id,
            cache_key = %request.cache_key,
            "Requesting fallback data from CBPro service"
        );
        let rpc_result = self
            .call_with_reconnect(&DEFAULT_RETRY_CONFIG, |client| {
                let r = request.clone();
                async move { client.get_fallback(tarpc::context::current(), r).await }
            })
            .await;

        match rpc_result {
            Ok(response) => {
                debug!(
                    available = response.available,
                    source = ?response.source,
                    "Received fallback response"
                );
                response
            }
            Err(e) => {
                warn!(error = %e, "CBPro service get_fallback failed after retries");
                FallbackResponse {
                    available: false,
                    data: None,
                    source: FallbackSource::None,
                    age_secs: None,
                    error: Some(RpcError::ServiceUnavailable {
                        service: mazbri_core::ServiceName::Cbpro.as_str().into(),
                        message: format!("CBPro service error: {e}"),
                    }),
                }
            }
        }
    }

    /// Lightweight connectivity probe (also used after reconnect to verify the channel).
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn health_check(&self) -> Result<(), String> {
        match self
            .call_with_reconnect(&DEFAULT_RETRY_CONFIG, |client| async move {
                client.health(tarpc::context::current()).await
            })
            .await
        {
            Ok(resp) => {
                debug!(status = %resp.status, service = %resp.service, "CBPro health ok");
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    pub async fn store_response(&self, request: StoreRequest) -> StoreResponse {
        debug!(
            product_id = %request.product_id,
            cache_key = %request.cache_key,
            "Storing fallback data to CBPro service"
        );
        let rpc_result = self
            .call_with_reconnect(&DEFAULT_RETRY_CONFIG, |client| {
                let r = request.clone();
                async move { client.store_response(tarpc::context::current(), r).await }
            })
            .await;

        match rpc_result {
            Ok(response) => {
                if response.success {
                    debug!(success = true, "CBPro store_response completed");
                } else {
                    warn!(
                        success = false,
                        error = ?response.error,
                        "CBPro store_response returned success=false"
                    );
                }
                response
            }
            Err(e) => {
                warn!(error = %e, "CBPro service store_response failed after retries");
                StoreResponse {
                    success: false,
                    cache_key: String::new(),
                    error: Some(RpcError::ServiceUnavailable {
                        service: mazbri_core::ServiceName::Cbpro.as_str().into(),
                        message: format!("CBPro service error: {e}"),
                    }),
                }
            }
        }
    }
}
