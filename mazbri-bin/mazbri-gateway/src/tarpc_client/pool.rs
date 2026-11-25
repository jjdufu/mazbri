use super::PooledClient;
use crate::config::TarpcConfig;
use dashmap::DashMap;
use mazbri_core::metrics::{RpcMetricsChannel, spawn_rpc_client};
use mazbri_core::{CoreError, ServiceName};
use mazbri_rpc::{
    AuthServiceClient, AuthServiceRequest, AuthServiceResponse, BillingServiceClient,
    BillingServiceRequest, BillingServiceResponse, CacheServiceClient, CacheServiceRequest,
    CacheServiceResponse, RateLimitServiceClient, RateLimitServiceRequest,
    RateLimitServiceResponse,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tarpc::client;
use tracing::info;

const CALLER: &str = ServiceName::Gateway.as_str();

type AuthClient = AuthServiceClient<RpcMetricsChannel<AuthServiceRequest, AuthServiceResponse>>;
type RateLimitClient =
    RateLimitServiceClient<RpcMetricsChannel<RateLimitServiceRequest, RateLimitServiceResponse>>;
type CacheClient = CacheServiceClient<RpcMetricsChannel<CacheServiceRequest, CacheServiceResponse>>;
type BillingClient =
    BillingServiceClient<RpcMetricsChannel<BillingServiceRequest, BillingServiceResponse>>;

macro_rules! impl_get_client {
    (
        $method_name:ident,
        $client_type:ty,
        $clients_field:ident,
        $locks_field:ident,
        $service_name:literal,
        $callee:expr
    ) => {
        /// # Errors
        ///
        /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
        pub async fn $method_name(&self, addr: &str) -> Result<$client_type, CoreError> {
            if let Some(entry) = self.$clients_field.get(addr)
                && !self.is_client_expired(entry.created_at)
            {
                return Ok(entry.client.clone());
            }
            let connect_gate = Arc::clone(
                self.$locks_field
                    .entry(addr.to_string())
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                    .value(),
            );
            let result = {
                let _guard = connect_gate.lock().await;
                if let Some(entry) = self.$clients_field.get(addr)
                    && !self.is_client_expired(entry.created_at)
                {
                    Ok(entry.client.clone())
                } else {
                    info!(addr = %addr, "Creating new tarpc {} client (mTLS)", $service_name);
                    let target = mazbri_tls::resolve_connect_target(addr)
                        .await
                        .map_err(mazbri_tls::TlsError::into_core)?;
                    let transport = mazbri_tls::tls_connect_with_server_name(
                        target.socket_addr,
                        self.tls_connector.clone(),
                        target.server_name.as_deref(),
                        tarpc::tokio_serde::formats::Bincode::default(),
                    )
                    .await
                    .map_err(|err| CoreError::Tls(err.to_string()))?;
                    let client: $client_type = spawn_rpc_client(
                        client::Config::default(),
                        transport,
                        CALLER,
                        $callee,
                    );
                    self.$clients_field
                        .insert(addr.to_string(), PooledClient::new(client.clone()));
                    info!(
                        addr = %addr,
                        server_name = ?target.server_name,
                        "tarpc {} client connected (mTLS)",
                        $service_name
                    );
                    Ok(client)
                }
            };
            self.$locks_field
                .remove_if(addr, |_, gate| Arc::strong_count(gate) <= 2);
            result
        }
    };
}

macro_rules! impl_cleanup {
    ($self:expr, $clients_field:ident) => {
        $self
            .$clients_field
            .iter()
            .filter(|entry| $self.is_client_expired_grace(entry.created_at))
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
    auth_clients: Arc<DashMap<String, PooledClient<AuthClient>>>,
    ratelimit_clients: Arc<DashMap<String, PooledClient<RateLimitClient>>>,
    cache_clients: Arc<DashMap<String, PooledClient<CacheClient>>>,
    billing_clients: Arc<DashMap<String, PooledClient<BillingClient>>>,
    auth_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    ratelimit_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    cache_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    billing_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    tls_connector: mazbri_tls::TlsConnector,
    client_ttl_secs: u64,
    auth_timeout: Duration,
}

impl TarpcClientPool {
    /// 绑定 `ManagedCertHandle` 构造动态热重载客户端池。
    ///
    /// # Errors
    ///
    /// 证书句柄未加载或 TLS 配置非法时返回错误。
    pub fn new_with_cert_handle(
        _tls_ca_cert: &str,
        cert_handle: &mazbri_tls::ManagedCertHandle,
        tls_config: &mazbri_tls::TlsConfig,
        tarpc_config: &TarpcConfig,
        auth_timeout_ms: u64,
        upstream_timeout_ms: u64,
    ) -> Result<Self, CoreError> {
        let tls_connector = mazbri_tls::TlsConnector::from_cert_handle(cert_handle, tls_config)
            .map_err(|e| CoreError::Tls(e.to_string()))?;
        let client_ttl_secs = tarpc_config.client_ttl_secs.max(1);
        let auth_timeout = Duration::from_millis(auth_timeout_ms.max(100));
        let upstream_timeout = Duration::from_millis(upstream_timeout_ms.max(100));
        info!(
            "Dynamic TLS connector created for tarpc client pool (mTLS mandatory) \
             - timeouts: auth={}ms, upstream={}ms, ttl={}s",
            auth_timeout.as_millis(),
            upstream_timeout.as_millis(),
            client_ttl_secs
        );
        Ok(Self {
            auth_clients: Arc::new(DashMap::new()),
            ratelimit_clients: Arc::new(DashMap::new()),
            cache_clients: Arc::new(DashMap::new()),
            billing_clients: Arc::new(DashMap::new()),
            auth_locks: Arc::new(DashMap::new()),
            ratelimit_locks: Arc::new(DashMap::new()),
            cache_locks: Arc::new(DashMap::new()),
            billing_locks: Arc::new(DashMap::new()),
            tls_connector,
            client_ttl_secs,
            auth_timeout,
        })
    }

    #[must_use]
    pub fn auth_rpc_context(&self) -> tarpc::context::Context {
        Self::rpc_context(self.auth_timeout)
    }

    #[must_use]
    pub fn rpc_context(timeout: Duration) -> tarpc::context::Context {
        let mut ctx = tarpc::context::current();
        ctx.deadline = Instant::now() + timeout.max(Duration::from_millis(50));
        ctx
    }

    fn is_client_expired(&self, created_at: Instant) -> bool {
        created_at.elapsed().as_secs() >= self.client_ttl_secs
    }

    fn is_client_expired_grace(&self, created_at: Instant) -> bool {
        created_at.elapsed().as_secs() >= self.client_ttl_secs.saturating_mul(2)
    }

    impl_get_client!(
        get_auth_client,
        AuthClient,
        auth_clients,
        auth_locks,
        "Auth",
        ServiceName::Auth.as_str()
    );
    impl_get_client!(
        get_ratelimit_client,
        RateLimitClient,
        ratelimit_clients,
        ratelimit_locks,
        "RateLimit",
        ServiceName::Ratelimit.as_str()
    );
    impl_get_client!(
        get_cache_client,
        CacheClient,
        cache_clients,
        cache_locks,
        "Cache",
        ServiceName::Cache.as_str()
    );
    impl_get_client!(
        get_billing_client,
        BillingClient,
        billing_clients,
        billing_locks,
        "Billing",
        ServiceName::Billing.as_str()
    );

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
        info!(addr = %addr, "Removed tarpc client");
    }

    pub fn cleanup_stale(&self) {
        let removed_count = impl_cleanup!(self, auth_clients)
            + impl_cleanup!(self, ratelimit_clients)
            + impl_cleanup!(self, cache_clients)
            + impl_cleanup!(self, billing_clients);
        if removed_count > 0 {
            info!("Cleaned up {} stale tarpc clients", removed_count);
        }
    }

    #[must_use]
    pub fn spawn_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let pool = self.clone();
        let ttl_secs = self.client_ttl_secs.max(1);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(ttl_secs));
            loop {
                interval.tick().await;
                pool.cleanup_stale();
            }
        })
    }
}
