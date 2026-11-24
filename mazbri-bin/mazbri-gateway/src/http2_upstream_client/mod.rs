use dashmap::DashMap;
use mazbri_core::CoreError;
use mazbri_tls::{ManagedCertificate, resolve_connect_target};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tonic::Request;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};
use tonic_prost::ProstCodec;

use crate::gateway::metrics::{
    GATEWAY_UPSTREAM_H2_CONNECT_SECONDS, GATEWAY_UPSTREAM_H2_CONNECTS_TOTAL,
    GATEWAY_UPSTREAM_H2_ERRORS_TOTAL, GATEWAY_UPSTREAM_H2_READY_WAIT_SECONDS,
    GATEWAY_UPSTREAM_H2_STREAMS_IN_USE, classify_h2_error,
};
use crate::upstream_contract::grpc_path_for_contract;
use mazbri_upstream_contracts::shared::{
    HttpUpstreamRequest, HttpUpstreamResponse, InvokeRequest, InvokeResponse,
};

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Default HTTP/2 connections per upstream endpoint; requests are spread round-robin so one
/// connection's head-of-line / single-task scheduling no longer bounds the hop tail.
pub const CHANNELS_PER_ENDPOINT: usize = 16;
/// Refresh starts this fraction (1/N) of the TTL before expiry, in the background.
const REFRESH_LEAD_DIVISOR: u64 = 10;

/// 单条 HTTP/2 长连接及其与对端 bypass 实例已确认的配置哈希账本。
///
/// 哈希随连接生命周期绑定：连接轮换新建时为空，传输层错误或重连迹象发生时清空，
/// 且仅在对端成功响应（非 `CONFIG_CACHE_MISS`）后写入确认状态。
pub struct PooledChannel {
    channel: Channel,
    synced_hashes: DashMap<Arc<str>, Arc<str>>,
}

impl PooledChannel {
    fn new(channel: Channel) -> Self {
        Self {
            channel,
            synced_hashes: DashMap::new(),
        }
    }

    #[must_use]
    pub fn is_synced(&self, key: &str, hash: &str) -> bool {
        self.synced_hashes
            .get(key)
            .is_some_and(|entry| entry.value().as_ref() == hash)
    }

    pub fn mark_synced(&self, key: Arc<str>, hash: Arc<str>) {
        self.synced_hashes.insert(key, hash);
    }

    pub fn invalidate_key(&self, key: &str) {
        self.synced_hashes.remove(key);
    }

    pub fn clear_synced(&self) {
        self.synced_hashes.clear();
    }
}

/// A set of channels to one endpoint plus round-robin cursor and refresh state.
struct ChannelSet {
    channels: Vec<Arc<PooledChannel>>,
    next: AtomicUsize,
    created_at: Instant,
    refreshing: AtomicBool,
}

impl ChannelSet {
    fn new(channels: Vec<Arc<PooledChannel>>) -> Self {
        Self {
            channels,
            next: AtomicUsize::new(0),
            created_at: Instant::now(),
            refreshing: AtomicBool::new(false),
        }
    }

    fn pick(&self) -> Arc<PooledChannel> {
        let i = self.next.fetch_add(1, Ordering::Relaxed) % self.channels.len();
        Arc::clone(&self.channels[i])
    }
}

/// Whether a set of the given age should be refreshed in the background.
fn needs_refresh(age_secs: u64, ttl_secs: u64) -> bool {
    let lead = (ttl_secs / REFRESH_LEAD_DIVISOR).max(1);
    age_secs.saturating_add(lead) >= ttl_secs
}

#[derive(Clone)]
pub struct UpstreamHttp2ClientPool {
    clients: Arc<DashMap<String, Arc<ChannelSet>>>,
    client_ttl_secs: u64,
    channels_per_endpoint: usize,
    client_cert: ManagedCertificate,
}

impl UpstreamHttp2ClientPool {
    #[must_use]
    pub fn new(client_cert: &ManagedCertificate, client_ttl_secs: u64) -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            client_ttl_secs,
            channels_per_endpoint: CHANNELS_PER_ENDPOINT,
            client_cert: client_cert.clone(),
        }
    }

    #[must_use]
    pub fn with_channels_per_endpoint(mut self, channels: usize) -> Self {
        self.channels_per_endpoint = channels.clamp(1, 128);
        self
    }

    /// Hot path: never rebuilds synchronously once a set exists. Near/after TTL the old
    /// set keeps serving while a background task builds and swaps in a new one.
    ///
    /// # Errors
    ///
    /// 首次建连或 TLS 握手失败时返回错误。
    pub async fn pick_slot(
        &self,
        contract_id: &str,
        endpoint: &str,
    ) -> Result<Arc<PooledChannel>, CoreError> {
        let set = self.clients.get(endpoint).map(|e| Arc::clone(e.value()));
        if let Some(set) = set {
            if needs_refresh(set.created_at.elapsed().as_secs(), self.client_ttl_secs)
                && !set.refreshing.swap(true, Ordering::AcqRel)
            {
                let pool = self.clone();
                let (contract, ep) = (contract_id.to_string(), endpoint.to_string());
                let old = Arc::clone(&set);
                tokio::spawn(async move {
                    match pool.connect_set(&contract, &ep, "refresh").await {
                        Ok(new_set) => {
                            pool.clients.insert(ep, Arc::new(new_set));
                        }
                        Err(error) => {
                            tracing::warn!(%error, endpoint = %ep, "background upstream refresh failed; keep old channels");
                            old.refreshing.store(false, Ordering::Release);
                        }
                    }
                });
            }
            return Ok(set.pick());
        }
        let new_set = Arc::new(self.connect_set(contract_id, endpoint, "initial").await?);
        let set = Arc::clone(
            self.clients
                .entry(endpoint.to_string())
                .or_insert(new_set)
                .value(),
        );
        Ok(set.pick())
    }

    async fn connect_set(
        &self,
        contract_id: &str,
        endpoint: &str,
        reason: &str,
    ) -> Result<ChannelSet, CoreError> {
        let count = self.channels_per_endpoint.max(1);
        let mut channels = Vec::with_capacity(count);
        for _ in 0..count {
            let started = Instant::now();
            let result = self.connect_one(contract_id, endpoint).await;
            let label = if result.is_ok() { "ok" } else { "error" };
            GATEWAY_UPSTREAM_H2_CONNECT_SECONDS
                .with_label_values(&[endpoint, label])
                .observe(started.elapsed().as_secs_f64());
            GATEWAY_UPSTREAM_H2_CONNECTS_TOTAL
                .with_label_values(&[endpoint, reason, label])
                .inc();
            channels.push(Arc::new(PooledChannel::new(result?)));
        }
        Ok(ChannelSet::new(channels))
    }

    async fn connect_one(&self, contract_id: &str, endpoint: &str) -> Result<Channel, CoreError> {
        let target = resolve_connect_target(endpoint)
            .await
            .map_err(mazbri_tls::TlsError::into_core)?;

        let server_name = target.server_name.clone().ok_or_else(|| {
            CoreError::Config(format!(
                "upstream endpoint {endpoint} requires a DNS server name for mTLS"
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
        let endpoint_obj = Endpoint::from_shared(endpoint_uri.clone())
            .map_err(|error| CoreError::Config(error.to_string()))?
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .tcp_nodelay(true)
            .http2_adaptive_window(true)
            .initial_connection_window_size(Some(4 * 1024 * 1024))
            .initial_stream_window_size(Some(1024 * 1024))
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

        tracing::info!(
            contract_id = contract_id,
            endpoint = %target.display_addr,
            server_name = %server_name,
            "upstream HTTP/2 client connected (mTLS)"
        );
        Ok(channel)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn warmup_endpoint(&self, endpoint: &str) -> Result<(), CoreError> {
        let _ = self.pick_slot("warmup", endpoint).await?;
        Ok(())
    }

    /// 按契约 ID 调用上游 Execute。
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn execute_request(
        &self,
        contract_id: &str,
        endpoint: &str,
        request: HttpUpstreamRequest,
    ) -> Result<HttpUpstreamResponse, CoreError> {
        let slot = self.pick_slot(contract_id, endpoint).await?;
        self.call_on_slot::<HttpUpstreamRequest, HttpUpstreamResponse>(
            &slot,
            contract_id,
            endpoint,
            request,
        )
        .await
    }

    /// 在指定连接槽位上执行 `BypassHttpProxyService/Execute`（用于按连接追踪配置同步状态）。
    ///
    /// # Errors
    ///
    /// 契约未知或传输层失败时返回错误（失败时自动清空该连接已确认哈希）。
    pub async fn execute_invoke_on_slot(
        &self,
        slot: &PooledChannel,
        contract_id: &str,
        endpoint: &str,
        request: InvokeRequest,
    ) -> Result<InvokeResponse, CoreError> {
        self.call_on_slot::<InvokeRequest, InvokeResponse>(slot, contract_id, endpoint, request)
            .await
    }

    /// Shared unary call with pool metrics: ready-wait, in-use streams, error kinds.
    async fn call_on_slot<Req, Resp>(
        &self,
        slot: &PooledChannel,
        contract_id: &str,
        endpoint: &str,
        request: Req,
    ) -> Result<Resp, CoreError>
    where
        Req: tonic_prost::prost::Message + Send + Sync + 'static,
        Resp: tonic_prost::prost::Message + Default + Send + Sync + 'static,
    {
        let grpc_path = grpc_path_for_contract(contract_id)?;
        let path = http::uri::PathAndQuery::from_static(grpc_path);

        let mut client = tonic::client::Grpc::new(slot.channel.clone());
        let ready_started = Instant::now();
        let ready = client.ready().await;
        GATEWAY_UPSTREAM_H2_READY_WAIT_SECONDS
            .with_label_values(&[endpoint])
            .observe(ready_started.elapsed().as_secs_f64());
        ready.map_err(|error| {
            slot.clear_synced();
            let msg = format!("{error:?}");
            GATEWAY_UPSTREAM_H2_ERRORS_TOTAL
                .with_label_values(&[endpoint, "ready", classify_h2_error(&msg)])
                .inc();
            CoreError::Network(error.to_string())
        })?;
        let codec = ProstCodec::<Req, Resp>::default();
        let in_use = GATEWAY_UPSTREAM_H2_STREAMS_IN_USE.with_label_values(&[endpoint]);
        in_use.inc();
        let response = client.unary(Request::new(request), path, codec).await;
        in_use.dec();
        let response = response.map_err(|status| {
            slot.clear_synced();
            let msg = format!("{} {:?}", status.message(), status);
            GATEWAY_UPSTREAM_H2_ERRORS_TOTAL
                .with_label_values(&[endpoint, "call", classify_h2_error(&msg)])
                .inc();
            CoreError::Network(status.to_string())
        })?;
        Ok(response.into_inner())
    }

    pub fn cleanup_stale(&self) {
        let removed_count = self
            .clients
            .iter()
            .filter(|entry| {
                entry.created_at.elapsed().as_secs() >= self.client_ttl_secs.saturating_mul(2)
            })
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

    #[must_use]
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

#[cfg(test)]
mod tests {
    use super::{PooledChannel, needs_refresh};
    use std::sync::Arc;
    use tonic::transport::Endpoint;

    fn dummy_slot() -> PooledChannel {
        let ch = Endpoint::from_static("http://127.0.0.1:9").connect_lazy();
        PooledChannel::new(ch)
    }

    #[test]
    fn refresh_starts_before_ttl() {
        assert!(!needs_refresh(0, 300));
        assert!(!needs_refresh(269, 300));
        assert!(needs_refresh(270, 300));
        assert!(needs_refresh(400, 300));
        assert!(needs_refresh(0, 0));
    }

    #[tokio::test]
    async fn pooled_channel_sync_tracks_confirm_invalidate_and_reconnect_clear() {
        let slot = dummy_slot();
        let key: Arc<str> = Arc::from("egress:ch-1");
        let h1: Arc<str> = Arc::from("hash-v1");
        let h2: Arc<str> = Arc::from("hash-v2");

        // Unconfirmed slot forces full send for all concurrent callers.
        assert!(!slot.is_synced(&key, &h1));
        assert!(!slot.is_synced(&key, &h1));

        slot.mark_synced(Arc::clone(&key), Arc::clone(&h1));
        assert!(slot.is_synced(&key, &h1));
        assert!(!slot.is_synced(&key, &h2));

        slot.invalidate_key(&key);
        assert!(!slot.is_synced(&key, &h1));

        slot.mark_synced(Arc::clone(&key), Arc::clone(&h2));
        assert!(slot.is_synced(&key, &h2));
        slot.clear_synced();
        assert!(!slot.is_synced(&key, &h2));
    }
}
