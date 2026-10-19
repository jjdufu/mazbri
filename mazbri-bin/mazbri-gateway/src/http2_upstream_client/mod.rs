use dashmap::DashMap;
use mazbri_core::{CoreError, PooledClient};
use mazbri_tls::{ManagedCertificate, resolve_connect_target};
use std::sync::Arc;
use std::time::Duration;
use tonic::Request;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};
use tonic_prost::ProstCodec;

use crate::upstream_contract::grpc_path_for_contract;
use mazbri_upstream_contracts::shared::{
    HttpUpstreamRequest, HttpUpstreamResponse, InvokeRequest, InvokeResponse,
};

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct UpstreamHttp2ClientPool {
    clients: Arc<DashMap<String, PooledClient<Channel>>>,
    client_ttl_secs: u64,
    client_cert: ManagedCertificate,
}

impl UpstreamHttp2ClientPool {
    pub fn new(client_cert: &ManagedCertificate, client_ttl_secs: u64) -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            client_ttl_secs,
            client_cert: client_cert.clone(),
        }
    }

    fn is_client_expired(&self, created_at: std::time::Instant) -> bool {
        created_at.elapsed().as_secs() >= self.client_ttl_secs
    }

    async fn get_channel(
        &self,
        contract_id: &str,
        endpoint: &str,
        server_name: Option<&str>,
    ) -> Result<Channel, CoreError> {
        // 连接池 cache key 仍按 endpoint（+server_name），与契约身份无关
        let cache_key = match server_name {
            Some(server_name) => format!("{}#{}", endpoint, server_name),
            None => endpoint.to_string(),
        };

        if let Some(entry) = self.clients.get(&cache_key) {
            if !self.is_client_expired(entry.created_at) {
                return Ok(entry.client.clone());
            }
            drop(entry);
            self.clients.remove(&cache_key);
        }

        let target = resolve_connect_target(endpoint, server_name)
            .await
            .map_err(|error| CoreError::Config(error.to_string()))?;

        let server_name = target.server_name.clone().ok_or_else(|| {
            CoreError::Config(format!(
                "upstream endpoint {} requires a DNS server name for mTLS",
                endpoint
            ))
        })?;

        let identity = Identity::from_pem(
            self.client_cert.cert_pem.as_bytes(),
            self.client_cert.key_pem.as_bytes(),
        );
        let ca_certificate = Certificate::from_pem(self.client_cert.ca_cert_pem.as_bytes());
        let tls_config = ClientTlsConfig::new()
            .identity(identity)
            .ca_certificate(ca_certificate)
            .domain_name(server_name.clone())
            .assume_http2(true)
            .timeout(DEFAULT_CONNECT_TIMEOUT);

        let endpoint_uri = format!("https://{}", target.display_addr);
        // 内网跳 keepalive：降低热路径重握手概率，配合启动预热压低 hop
        let endpoint_obj = Endpoint::from_shared(endpoint_uri.clone())
            .map_err(|error| CoreError::Config(error.to_string()))?
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .http2_keep_alive_interval(Duration::from_secs(20))
            .keep_alive_timeout(Duration::from_secs(10))
            .keep_alive_while_idle(true)
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .tls_config(tls_config)
            .map_err(|error| CoreError::Tls(error.to_string()))?;

        tracing::info!(
            contract_id = contract_id,
            endpoint = %endpoint_uri,
            server_name = %server_name,
            "Creating new upstream HTTP/2 client (mTLS)"
        );

        let channel = endpoint_obj
            .connect()
            .await
            .map_err(|error| CoreError::Network(error.to_string()))?;

        self.clients
            .insert(cache_key, PooledClient::new(channel.clone()));

        tracing::info!(
            contract_id = contract_id,
            endpoint = %target.display_addr,
            server_name = %server_name,
            "upstream HTTP/2 client connected (mTLS)"
        );
        Ok(channel)
    }

    pub async fn warmup_endpoint(
        &self,
        endpoint: &str,
        server_name: Option<&str>,
    ) -> Result<(), CoreError> {
        let _ = self.get_channel("warmup", endpoint, server_name).await?;
        Ok(())
    }

    /// 按契约 ID 调用上游 Execute。
    ///
    /// `contract_id` 必须是已注册契约（见 `upstream_contract`），不是业务 channel_code。
    pub async fn execute_request(
        &self,
        contract_id: &str,
        endpoint: &str,
        server_name: Option<&str>,
        request: HttpUpstreamRequest,
    ) -> Result<HttpUpstreamResponse, CoreError> {
        let channel = self.get_channel(contract_id, endpoint, server_name).await?;
        let grpc_path = grpc_path_for_contract(contract_id)?;
        let path = http::uri::PathAndQuery::from_static(grpc_path);

        let mut client = tonic::client::Grpc::new(channel);
        client
            .ready()
            .await
            .map_err(|error| CoreError::Network(error.to_string()))?;
        let codec = ProstCodec::<HttpUpstreamRequest, HttpUpstreamResponse>::default();
        let request = Request::new(request);

        let response = client
            .unary(request, path, codec)
            .await
            .map_err(|error| CoreError::Network(error.to_string()))?;

        Ok(response.into_inner())
    }

    /// Code-free invoke: BypassHttpProxyService/Execute with InvokeRequest.
    pub async fn execute_invoke(
        &self,
        contract_id: &str,
        endpoint: &str,
        server_name: Option<&str>,
        request: InvokeRequest,
    ) -> Result<InvokeResponse, CoreError> {
        let channel = self.get_channel(contract_id, endpoint, server_name).await?;
        let grpc_path = grpc_path_for_contract(contract_id)?;
        let path = http::uri::PathAndQuery::from_static(grpc_path);

        let mut client = tonic::client::Grpc::new(channel);
        client
            .ready()
            .await
            .map_err(|error| CoreError::Network(error.to_string()))?;
        let codec = ProstCodec::<InvokeRequest, InvokeResponse>::default();
        let request = Request::new(request);

        let response = client
            .unary(request, path, codec)
            .await
            .map_err(|error| CoreError::Network(error.to_string()))?;

        Ok(response.into_inner())
    }

    pub fn cleanup_stale(&self) {
        let removed_count = self
            .clients
            .iter()
            .filter(|entry| self.is_client_expired(entry.created_at))
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>()
            .into_iter()
            .inspect(|key| {
                self.clients.remove(key);
            })
            .count();
        if removed_count > 0 {
            tracing::info!(removed_count, "Cleaned up stale upstream HTTP/2 clients");
        }
    }

    pub fn spawn_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let pool = self.clone();
        let ttl_secs = self.client_ttl_secs;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(ttl_secs));
            loop {
                interval.tick().await;
                pool.cleanup_stale();
            }
        })
    }
}
