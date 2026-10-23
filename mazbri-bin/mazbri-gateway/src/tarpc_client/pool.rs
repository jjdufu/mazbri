use super::PooledClient;
use crate::config::TarpcConfig;
use dashmap::DashMap;
use mazbri_core::CoreError;
use mazbri_rpc::{
    AuthServiceClient, BillingServiceClient, CacheServiceClient, HttpUpstreamServiceClient,
    RateLimitServiceClient,
};
use mazbri_tls::ManagedCertificate;
use std::sync::Arc;
use tarpc::client;
use tracing::info;
const HTTP_UPSTREAM_CLIENT_POOL_SIZE: usize = 8;
macro_rules! impl_get_client {
    (
        $method_name:ident, $client_type:ty, $clients_field:ident, $service_name:literal
    ) => {
        /// # Errors
        ///
        /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
        pub async fn $method_name(&self, addr: &str) -> Result<$client_type, CoreError> {
            if let Some(entry) = self.$clients_field.get(addr) {
                if !self.is_client_expired(entry.created_at) {
                    return Ok(entry.client.clone());
                }
                drop(entry);
                self.$clients_field.remove(addr);
            }
            info!(addr = %addr, "Creating new tarpc {} client (mTLS)", $service_name);
            let target = mazbri_tls::resolve_connect_target(addr)
                .await
                .map_err(|e| CoreError::Config(e.to_string()))?;
            let transport = mazbri_tls::tls_connect_with_server_name(
                target.socket_addr,
                self.tls_connector.clone(),
                target.server_name.as_deref(),
                tarpc::tokio_serde::formats::Bincode::default(),
            )
            .await
            .map_err(|err| CoreError::Tls(err.to_string()))?;
            let client = <$client_type>::new(client::Config::default(), transport).spawn();
            self.$clients_field
                .insert(addr.to_string(), PooledClient::new(client.clone()));
            info!(
                addr = %addr, server_name = ?target.server_name,
                "tarpc {} client connected (mTLS)", $service_name
            );
            Ok(client)
        }
    };
}
macro_rules! impl_cleanup {
    ($self:expr, $clients_field:ident) => {
        $self
            .$clients_field
            .iter()
            .filter(|entry| $self.is_client_expired(entry.created_at))
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>()
            .into_iter()
            .inspect(|key| {
                $self.$clients_field.remove(key);
            })
            .count()
    };
}
#[derive(Clone)]
pub struct TarpcClientPool {
    auth_clients: Arc<DashMap<String, PooledClient<AuthServiceClient>>>,
    ratelimit_clients: Arc<DashMap<String, PooledClient<RateLimitServiceClient>>>,
    cache_clients: Arc<DashMap<String, PooledClient<CacheServiceClient>>>,
    billing_clients: Arc<DashMap<String, PooledClient<BillingServiceClient>>>,
    http_upstream_clients: Arc<DashMap<String, PooledClient<HttpUpstreamServiceClient>>>,
    http_upstream_round_robin: Arc<DashMap<String, usize>>,
    tls_connector: mazbri_tls::TlsConnector,
    client_ttl_secs: u64,
}
impl TarpcClientPool {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn new_with_timeouts(
        tls_ca_cert: &str,
        client_cert: &ManagedCertificate,
        tls_config: &mazbri_tls::TlsConfig,
        tarpc_config: &TarpcConfig,
        auth_timeout_ms: u64,
        upstream_timeout_ms: u64,
    ) -> Result<Self, CoreError> {
        let tls_connector =
            mazbri_tls::TlsConnector::new(tls_ca_cert, Some(client_cert), tls_config)
                .map_err(|e| CoreError::Tls(e.to_string()))?;
        info!(
            "TLS connector created for tarpc client pool (mTLS mandatory) \
             - timeouts: auth={}ms, upstream={}ms, ttl={}s",
            auth_timeout_ms, upstream_timeout_ms, tarpc_config.client_ttl_secs
        );
        Ok(Self {
            auth_clients: Arc::new(DashMap::new()),
            ratelimit_clients: Arc::new(DashMap::new()),
            cache_clients: Arc::new(DashMap::new()),
            billing_clients: Arc::new(DashMap::new()),
            http_upstream_clients: Arc::new(DashMap::new()),
            http_upstream_round_robin: Arc::new(DashMap::new()),
            tls_connector,
            client_ttl_secs: tarpc_config.client_ttl_secs,
        })
    }
    fn is_client_expired(&self, created_at: std::time::Instant) -> bool {
        created_at.elapsed().as_secs() >= self.client_ttl_secs
    }
    impl_get_client!(get_auth_client, AuthServiceClient, auth_clients, "Auth");
    impl_get_client!(
        get_ratelimit_client,
        RateLimitServiceClient,
        ratelimit_clients,
        "RateLimit"
    );
    impl_get_client!(get_cache_client, CacheServiceClient, cache_clients, "Cache");
    impl_get_client!(
        get_billing_client,
        BillingServiceClient,
        billing_clients,
        "Billing"
    );
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_http_upstream_client(
        &self,
        addr: &str,
    ) -> Result<HttpUpstreamServiceClient, CoreError> {
        let slot = self.next_http_upstream_slot(addr);
        let cache_key = format!("{addr}|{slot}");
        if let Some(entry) = self.http_upstream_clients.get(&cache_key) {
            if !self.is_client_expired(entry.created_at) {
                return Ok(entry.client.clone());
            }
            drop(entry);
            self.http_upstream_clients.remove(&cache_key);
        }
        info!(
            addr = % addr, slot,
            "Creating new tarpc HttpUpstream client (Bincode over mTLS)"
        );
        let target = mazbri_tls::resolve_connect_target(addr)
            .await
            .map_err(|e| CoreError::Config(e.to_string()))?;
        let client = HttpUpstreamServiceClient::new(
            client::Config::default(),
            mazbri_tls::tls_connect_with_server_name(
                target.socket_addr,
                self.tls_connector.clone(),
                target.server_name.as_deref(),
                tarpc::tokio_serde::formats::Bincode::default(),
            )
            .await
            .map_err(|err| CoreError::Tls(err.to_string()))?,
        )
        .spawn();
        self.http_upstream_clients
            .insert(cache_key, PooledClient::new(client.clone()));
        info!(
            addr = % addr, server_name = ? target.server_name, slot,
            "tarpc HttpUpstream client connected (Bincode over mTLS)"
        );
        Ok(client)
    }
    fn next_http_upstream_slot(&self, base_cache_key: &str) -> usize {
        let mut entry = self
            .http_upstream_round_robin
            .entry(base_cache_key.to_string())
            .or_insert(0);
        let slot = *entry % HTTP_UPSTREAM_CLIENT_POOL_SIZE;
        *entry = entry.wrapping_add(1);
        slot
    }
    pub fn remove_client(&self, addr: &str) {
        fn remove_matching_keys<C>(cache: &DashMap<String, PooledClient<C>>, addr: &str) {
            let pooled_prefix = format!("{addr}|");
            let keys = cache
                .iter()
                .filter_map(|entry| {
                    if entry.key() == addr || entry.key().starts_with(&pooled_prefix) {
                        Some(entry.key().clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            for key in keys {
                cache.remove(&key);
            }
        }
        remove_matching_keys(&self.auth_clients, addr);
        remove_matching_keys(&self.ratelimit_clients, addr);
        remove_matching_keys(&self.cache_clients, addr);
        remove_matching_keys(&self.billing_clients, addr);
        remove_matching_keys(&self.http_upstream_clients, addr);
        self.http_upstream_round_robin.remove(addr);
        info!(addr = % addr, "Removed tarpc client");
    }
    pub fn cleanup_stale(&self) {
        let removed_count = impl_cleanup!(self, auth_clients)
            + impl_cleanup!(self, ratelimit_clients)
            + impl_cleanup!(self, cache_clients)
            + impl_cleanup!(self, billing_clients)
            + impl_cleanup!(self, http_upstream_clients);
        if removed_count > 0 {
            info!("Cleaned up {} stale tarpc clients", removed_count);
        }
    }
    #[must_use]
    pub fn spawn_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let pool = self.clone();
        let ttl_secs = self.client_ttl_secs;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(ttl_secs));
            loop {
                interval.tick().await;
                pool.cleanup_stale();
            }
        })
    }
}
