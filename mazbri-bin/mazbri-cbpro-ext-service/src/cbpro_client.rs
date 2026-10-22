use crate::config::ServiceEndpoint;
use mazbri_core::{CoreError, DEFAULT_RETRY_CONFIG, retry_rpc};
use mazbri_rpc::{
    CBProServiceClient, FallbackRequest, FallbackResponse, FallbackSource, RpcError, StoreRequest,
    StoreResponse,
};
use mazbri_tls::ManagedCertificate;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

#[derive(Clone)]
pub struct CBProClient {
    endpoint: ServiceEndpoint,
    client: Arc<RwLock<Option<CBProServiceClient>>>,
    tls_connector: mazbri_tls::TlsConnector,
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
    ) -> Result<Self, CoreError> {
        let tls_connector =
            mazbri_tls::TlsConnector::new(tls_ca_cert, Some(client_cert), tls_config)
                .map_err(|error| CoreError::Tls(error.to_string()))?;
        Ok(Self {
            endpoint,
            client: Arc::new(RwLock::new(None)),
            tls_connector,
        })
    }

    async fn get_client(&self) -> Result<CBProServiceClient, CoreError> {
        {
            let guard = self.client.read().await;
            if let Some(client) = guard.as_ref() {
                debug!(url = % self.endpoint.url(), "Using cached CBPro client");
                return Ok(client.clone());
            }
        }
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
            CBProServiceClient::new(tarpc::client::Config::default(), transport).spawn();
        {
            let mut guard = self.client.write().await;
            *guard = Some(new_client.clone());
        }
        Ok(new_client)
    }

    pub async fn get_fallback(&self, request: FallbackRequest) -> FallbackResponse {
        match self.get_client().await {
            Ok(client) => {
                let rpc_result = retry_rpc(&DEFAULT_RETRY_CONFIG, || {
                    let c = client.clone();
                    let r = request.clone();
                    async move { c.get_fallback(tarpc::context::current(), r).await }
                })
                .await;
                match rpc_result {
                    Ok(response) => response,
                    Err(error) => {
                        warn!(error = % error, "CBPro get_fallback failed");
                        FallbackResponse {
                            available: false,
                            data: None,
                            source: FallbackSource::None,
                            age_secs: None,
                            error: Some(RpcError::ServiceUnavailable {
                                service: mazbri_core::ServiceName::Cbpro.as_str().into(),
                                message: format!("CBPro service error: {error}"),
                            }),
                        }
                    }
                }
            }
            Err(error) => {
                warn!(error = % error, "Failed to connect to CBPro service");
                FallbackResponse {
                    available: false,
                    data: None,
                    source: FallbackSource::None,
                    age_secs: None,
                    error: Some(RpcError::ServiceUnavailable {
                        service: mazbri_core::ServiceName::Cbpro.as_str().into(),
                        message: format!("CBPro connection failed: {error}"),
                    }),
                }
            }
        }
    }

    pub async fn store_response(&self, request: StoreRequest) -> StoreResponse {
        match self.get_client().await {
            Ok(client) => {
                let rpc_result = retry_rpc(&DEFAULT_RETRY_CONFIG, || {
                    let c = client.clone();
                    let r = request.clone();
                    async move { c.store_response(tarpc::context::current(), r).await }
                })
                .await;
                match rpc_result {
                    Ok(response) => response,
                    Err(error) => {
                        warn!(error = % error, "CBPro store_response failed");
                        StoreResponse {
                            success: false,
                            cache_key: String::new(),
                            error: Some(RpcError::ServiceUnavailable {
                                service: mazbri_core::ServiceName::Cbpro.as_str().into(),
                                message: format!("CBPro service error: {error}"),
                            }),
                        }
                    }
                }
            }
            Err(error) => {
                warn!(error = % error, "Failed to connect to CBPro service");
                StoreResponse {
                    success: false,
                    cache_key: String::new(),
                    error: Some(RpcError::ServiceUnavailable {
                        service: mazbri_core::ServiceName::Cbpro.as_str().into(),
                        message: format!("CBPro connection failed: {error}"),
                    }),
                }
            }
        }
    }
}
