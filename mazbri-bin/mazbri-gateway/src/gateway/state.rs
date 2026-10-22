pub use super::validation::GatewayRequest;
use crate::cbpro_client::CBProClient;
use crate::cbpro_ext_client::CBProExtClient;
use crate::circuit_breaker::CircuitBreaker;
use crate::config::ChannelRateLimitConfig;
use crate::config::ServiceEndpoint;
use crate::crypto::TenantCryptoState;
use crate::http2_upstream_client::UpstreamHttp2ClientPool;
use crate::ratelimit::{BandwidthPrepaid, ChannelRateLimitEngine, LocalRateLimitEngine};
use crate::tarpc_client::TarpcClientPool;
use mazbri_product::ProductService;
use mazbri_routing::RouteEngine;
use mazbri_rpc::{ObservationRequest, RuntimeDecision};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct GatewayRequestTiming {
    pub started_at: Instant,
    pub auth_ms: u64,
    pub ratelimit_ms: u64,
    pub balance_check_ms: u64,
    pub tenant_validation_ms: u64,
    pub product_detail_lookup_ms: u64,
    pub upstream_ms: u64,
    pub post_response_ms: u64,
    /// Populated by ratelimit middleware for egress bandwidth settle + usage bytes.
    pub bandwidth_prepaid: Option<BandwidthPrepaid>,
}
impl GatewayRequestTiming {
    #[must_use]
    pub fn new(started_at: Instant) -> Self {
        Self {
            started_at,
            auth_ms: 0,
            ratelimit_ms: 0,
            balance_check_ms: 0,
            tenant_validation_ms: 0,
            product_detail_lookup_ms: 0,
            upstream_ms: 0,
            post_response_ms: 0,
            bandwidth_prepaid: None,
        }
    }
}
pub struct ProxyStateInit<'a> {
    pub service_name: String,
    pub product_service: Arc<ProductService>,
    pub tarpc_pool: TarpcClientPool,
    pub upstream_http2_pool: UpstreamHttp2ClientPool,
    pub circuit_breaker_config: crate::config::CircuitBreakerCacheConfig,
    pub ratelimit_endpoint: ServiceEndpoint,
    pub channel_rate_limit: ChannelRateLimitConfig,
    pub cbpro_client: Option<CBProClient>,
    pub cbpro_ext_client: Option<CBProExtClient>,
    pub route_engine: Arc<arc_swap::ArcSwap<RouteEngine>>,
    pub db: Arc<PgPool>,
    pub redis: mazbri_core::redis::RedisClient,
    pub cert_service_url: &'a str,
    pub cert_service_token: Option<String>,
    pub services_config: crate::config::ServicesConfig,
    pub supplier_config_send_cache: crate::supplier_config_cache::SupplierConfigSendCache,
}
#[derive(Clone)]
pub struct ProxyState {
    pub service_name: Arc<str>,
    pub product_service: Arc<ProductService>,
    pub circuit_breakers: moka::sync::Cache<String, Arc<CircuitBreaker>>,
    pub circuit_breaker_config: crate::config::CircuitBreakerCacheConfig,
    pub tarpc_pool: TarpcClientPool,
    pub upstream_http2_pool: UpstreamHttp2ClientPool,
    pub ratelimit_endpoint: ServiceEndpoint,
    pub cbpro_client: Option<CBProClient>,
    pub cbpro_ext_client: Option<CBProExtClient>,
    pub cbpro_runtime_decision_cache: moka::sync::Cache<String, RuntimeDecision>,
    pub cbpro_observation_tx: Option<tokio::sync::mpsc::Sender<ObservationRequest>>,
    pub route_engine: Arc<arc_swap::ArcSwap<RouteEngine>>,
    pub db: Arc<PgPool>,
    pub local_ratelimiter: Arc<LocalRateLimitEngine>,
    pub channel_ratelimiter: Arc<ChannelRateLimitEngine>,
    pub crypto_state: TenantCryptoState,
    pub services_config: crate::config::ServicesConfig,
    pub supplier_config_send_cache: crate::supplier_config_cache::SupplierConfigSendCache,
}
impl ProxyState {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn new(init: ProxyStateInit<'_>) -> anyhow::Result<Self> {
        let service_name = init.service_name;
        let circuit_breakers = moka::sync::Cache::builder()
            .max_capacity(init.circuit_breaker_config.max_cache_size as u64)
            .time_to_idle(std::time::Duration::from_secs(600))
            .build();
        let cbpro_runtime_decision_cache = moka::sync::Cache::builder()
            .max_capacity(10_000)
            .time_to_live(std::time::Duration::from_secs(3))
            .build();
        let crypto_state = TenantCryptoState::new(
            init.db.clone(),
            init.redis,
            init.cert_service_url,
            init.cert_service_token,
        )?;
        let (cbpro_observation_tx, cbpro_observation_rx) = if init.cbpro_ext_client.is_some() {
            let (tx, rx) = tokio::sync::mpsc::channel(1024);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let local_ratelimiter = Arc::new(LocalRateLimitEngine::new(service_name.clone()));
        let channel_ratelimiter = Arc::new(ChannelRateLimitEngine::new(init.channel_rate_limit));
        let state = Self {
            service_name: service_name.into(),
            product_service: init.product_service,
            circuit_breakers,
            circuit_breaker_config: init.circuit_breaker_config,
            tarpc_pool: init.tarpc_pool,
            upstream_http2_pool: init.upstream_http2_pool,
            ratelimit_endpoint: init.ratelimit_endpoint,
            cbpro_client: init.cbpro_client,
            cbpro_ext_client: init.cbpro_ext_client,
            cbpro_runtime_decision_cache,
            cbpro_observation_tx,
            route_engine: init.route_engine,
            db: init.db,
            local_ratelimiter,
            channel_ratelimiter,
            crypto_state,
            services_config: init.services_config,
            supplier_config_send_cache: init.supplier_config_send_cache,
        };
        if let Err(error) = state.crypto_state.warm_start().await {
            tracing::warn!(error = %error, "Gateway tenant crypto warmup failed");
        } else {
            tracing::info!("Gateway tenant crypto warmup completed");
        }
        let _crypto_bg_tasks = state.crypto_state.spawn_background_tasks();
        state
            .local_ratelimiter
            .clone()
            .warm_start(&state.tarpc_pool, &state.ratelimit_endpoint)
            .await;
        let _bg_tasks = state
            .local_ratelimiter
            .clone()
            .spawn_background_tasks(&state.tarpc_pool.clone(), &state.ratelimit_endpoint.clone());
        let _channel_rl_janitor = state.channel_ratelimiter.clone().spawn_background_tasks();
        if let (Some(client), Some(mut rx)) = (state.cbpro_ext_client.clone(), cbpro_observation_rx)
        {
            tokio::spawn(async move {
                let mut batch = Vec::with_capacity(32);
                loop {
                    let Some(first) = rx.recv().await else {
                        break;
                    };
                    batch.push(first);
                    while batch.len() < 32 {
                        match rx.try_recv() {
                            Ok(next) => batch.push(next),
                            Err(
                                tokio::sync::mpsc::error::TryRecvError::Empty
                                | tokio::sync::mpsc::error::TryRecvError::Disconnected,
                            ) => break,
                        }
                    }
                    let observations = std::mem::take(&mut batch);
                    client.record_observations(observations).await;
                }
            });
        }
        Ok(state)
    }
}
