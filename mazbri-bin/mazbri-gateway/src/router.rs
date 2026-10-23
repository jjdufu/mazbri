use crate::auth::{AuthService, AuthState};
use crate::balance_cache::{BalanceCacheConfig, BalancePrecheckCache, SharedBalancePrecheckCache};
use crate::cbpro_client::CBProClient;
use crate::cbpro_ext_client::CBProExtClient;
use crate::config::GatewayConfig;
use crate::gateway::disconnect::client_disconnect_middleware;
use crate::gateway::metrics::{GATEWAY_OVERLOAD_REJECT_TOTAL, GATEWAY_PANIC_TOTAL};
use crate::gateway::response::ProxyError;
use crate::gateway::{ProxyState, metrics_handler, unified_gateway_handler};
use crate::health::{health_check, ready_check};
use crate::http2_upstream_client::UpstreamHttp2ClientPool;
use crate::service_health_cache::ServiceHealthState;
use crate::tarpc_client::TarpcClientPool;
use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use mazbri_admin::AdminState;
use mazbri_billing::{BillingMiddlewareState, BillingService, BillingServiceConfig, ChargeQueue};
use mazbri_cache::RedisPermissionCache;
use mazbri_callback::CallbackWorker;
use mazbri_core::CoreError;
use mazbri_product::{ProductConfig, ProductService};
use mazbri_service::{error_handler_middleware, metrics_middleware, request_id_middleware};
use sqlx::PgPool;
use std::future::Future;
use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore};
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};
#[derive(Clone)]
pub struct AppState {
    pub proxy_state: ProxyState,
    pub auth_state: AuthState,
    pub auth_service: Arc<RwLock<AuthService>>,
    pub billing_state: Option<BillingMiddlewareState>,
    /// 余额预检 L1+L2 缓存（可选）
    pub balance_cache: Option<SharedBalancePrecheckCache>,
    /// Balance precheck hard timeout (ms). Never block proxy for tens of seconds.
    pub balance_check_timeout_ms: u64,
    /// On precheck infra failure: allow request (true) or return 503 (false).
    pub balance_check_fail_open: bool,
    pub service_health_cache: Option<crate::service_health_cache::ServiceHealthCache>,
    pub admin_state: Option<Arc<AdminState>>,
    pub request_binding: crate::config::RequestBindingConfig,
    /// L1: synchronous Redpanda producer for usage events (None = outbox path).
    pub usage_event_producer: Option<Arc<mazbri_billing::UsageEventProducer>>,
}
pub struct RouterInit<'a> {
    pub service_name: String,
    pub config: GatewayConfig,
    pub db: Arc<PgPool>,
    pub tls_ca_cert: String,
    pub client_cert: mazbri_tls::ManagedCertificate,
    pub tls_config: &'a mazbri_tls::TlsConfig,
    pub jwt_secret: String,
    pub admin_state: Option<Arc<AdminState>>,
    pub service_health_cache: Option<crate::service_health_cache::ServiceHealthCache>,
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_router(init: RouterInit<'_>) -> anyhow::Result<Router> {
    let RouterInit {
        service_name,
        config,
        db,
        tls_ca_cert,
        client_cert,
        tls_config,
        jwt_secret,
        admin_state,
        service_health_cache,
    } = init;
    let cors_layer = build_cors_layer(&config.cors);
    let max_connections = config.server.max_connections.max(1) as usize;
    let concurrency_limit = Arc::new(Semaphore::new(max_connections));
    let compression_layer = compression_layer_for(&config, max_connections);
    let base_middleware = ServiceBuilder::new()
        .layer(CatchPanicLayer::custom(handle_panic))
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(error_handler_middleware))
        .layer(axum::middleware::from_fn(normalize_limit_errors_middleware))
        .layer(axum::middleware::from_fn(metrics_middleware))
        .layer(axum::middleware::from_fn(request_id_middleware))
        .layer(axum::middleware::from_fn(client_disconnect_middleware))
        .layer(axum::middleware::from_fn_with_state(
            concurrency_limit,
            concurrency_limit_middleware,
        ))
        .layer(RequestBodyLimitLayer::new(
            crate::gateway::validation::MAX_BODY_SIZE,
        ))
        .layer(cors_layer)
        .layer(compression_layer);
    let redis = mazbri_core::redis::RedisClient::connect_with_config(&config.redis)
        .await
        .map_err(|error| anyhow::anyhow!("Failed to connect to Redis: {error}"))?;
    let product_service = init_product_service(&db, &redis).await;
    let (tarpc_pool, upstream_http2_pool) =
        init_client_pools(&config, &tls_ca_cert, &client_cert, tls_config)?;
    let route_engine =
        load_routes_and_warmup(&db, &tarpc_pool, &upstream_http2_pool, &config).await?;
    let route_engine = Arc::new(arc_swap::ArcSwap::new(Arc::new(route_engine)));
    let proxy_state = ProxyState::new(crate::gateway::ProxyStateInit {
        service_name,
        product_service,
        tarpc_pool: tarpc_pool.clone(),
        upstream_http2_pool: upstream_http2_pool.clone(),
        circuit_breaker_config: config.circuit_breaker.clone(),
        ratelimit_endpoint: config.services.ratelimit.clone(),
        channel_rate_limit: config.channel_rate_limit.clone(),
        cbpro_client: init_cbpro_client(&config, &tls_ca_cert, &client_cert, tls_config),
        cbpro_ext_client: init_cbpro_ext_client(&config, &tls_ca_cert, &client_cert, tls_config),
        route_engine: route_engine.clone(),
        db: db.clone(),
        redis: redis.clone(),
        cert_service_url: &tls_config.cert_service_url,
        cert_service_token: tls_config.cert_service_token.clone(),
        services_config: config.services.clone(),
        supplier_config_send_cache: crate::supplier_config_cache::SupplierConfigSendCache::new(),
    })
    .await?;
    if let Some(cache) = service_health_cache.clone() {
        spawn_ratelimit_health_monitor(
            cache,
            proxy_state.tarpc_pool.clone(),
            config.services.ratelimit.clone(),
        );
    }
    let route_reload_status = spawn_background_workers(
        &config,
        &db,
        &route_engine,
        admin_state.as_ref(),
        &proxy_state,
    );
    let (auth_state, auth_service) = init_auth(&config, &db, jwt_secret, tarpc_pool).await;
    let billing_state = create_billing_state(&config, &db).await;
    let balance_cache = build_balance_cache(&config, &redis);
    let usage_event_producer = build_usage_event_producer(&config).await;
    let app_state = AppState {
        proxy_state,
        auth_state,
        auth_service,
        billing_state,
        balance_cache,
        balance_check_timeout_ms: config.billing.balance_check.timeout_ms.max(50),
        balance_check_fail_open: config.billing.balance_check.fail_open,
        service_health_cache,
        admin_state,
        request_binding: config.request_binding.clone(),
        usage_event_producer,
    };
    let public_routes = build_public_routes(route_reload_status);
    let protected_routes = build_protected_routes(&app_state, &config);
    Ok(Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(base_middleware)
        .with_state(app_state))
}
async fn init_product_service(
    db: &PgPool,
    redis: &mazbri_core::redis::RedisClient,
) -> Arc<ProductService> {
    let product_config = ProductConfig::default();
    let permission_cache = Arc::new(RedisPermissionCache::new(
        redis.clone(),
        Duration::from_secs(product_config.cache_ttl_secs),
    ));
    let product_service = Arc::new(ProductService::new(
        db.clone(),
        product_config,
        permission_cache,
    ));
    if let Err(error) = product_service.warm_tenant_product_detail_cache().await {
        tracing::warn!(error = % error, "Product tenant detail cache warmup failed");
    }
    product_service
}

fn init_client_pools(
    config: &GatewayConfig,
    tls_ca_cert: &str,
    client_cert: &mazbri_tls::ManagedCertificate,
    tls_config: &mazbri_tls::TlsConfig,
) -> anyhow::Result<(TarpcClientPool, UpstreamHttp2ClientPool)> {
    let tarpc_pool = TarpcClientPool::new_with_timeouts(
        tls_ca_cert,
        client_cert,
        tls_config,
        &config.tarpc,
        config.services.auth.timeout_ms,
        30_000,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create TarpcClientPool: {e}"))?;
    // 后台清理任务随进程存活，句柄无需持有（drop 即 detach）。
    drop(tarpc_pool.spawn_cleanup_task());
    tracing::info!("TarpcClientPool background cleanup task started");
    let upstream_http2_pool =
        UpstreamHttp2ClientPool::new(client_cert, config.tarpc.client_ttl_secs);
    drop(upstream_http2_pool.spawn_cleanup_task());
    tracing::info!("UpstreamHttp2ClientPool background cleanup task started");
    Ok((tarpc_pool, upstream_http2_pool))
}

async fn init_auth(
    config: &GatewayConfig,
    db: &PgPool,
    jwt_secret: String,
    tarpc_pool: TarpcClientPool,
) -> (AuthState, Arc<RwLock<AuthService>>) {
    let extra_jwt_keys = crate::crypto::load_platform_hs256_jwt_keys(db)
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(error = %error, "Failed to load extra platform HS256 JWT keys");
            Vec::new()
        });
    let jwt_keyring = crate::auth::JwtHs256Keyring::new(config.auth.jwt_kid.clone(), jwt_secret)
        .with_extra_keys(extra_jwt_keys);
    let auth_service = Arc::new(RwLock::new(AuthService::new(
        jwt_keyring.clone(),
        config.auth.token_expiration_secs,
        tarpc_pool,
        config.services.auth.clone(),
        db.clone(),
    )));
    let auth_state = AuthState::new(jwt_keyring);
    (auth_state, auth_service)
}

fn build_public_routes(
    route_reload_status: Arc<crate::route_reload::RouteReloadStatus>,
) -> Router<AppState> {
    Router::new()
        .route("/health", get(health_check))
        .route("/ready", get(ready_check))
        .route("/metrics", get(metrics_handler))
        .route(
            "/admin/routes/status",
            get(move || {
                let status = route_reload_status.clone();
                async move { axum::Json(status.snapshot()) }
            }),
        )
        .route("/auth/token", post(crate::auth::service::token_handler))
        .route("/openapi.yaml", get(crate::openapi::openapi_yaml))
}

fn compression_layer_for(config: &GatewayConfig, max_connections: usize) -> CompressionLayer {
    tracing::info!(
        max_connections,
        tcp_nodelay = config.server.tcp_nodelay,
        tcp_keepalive_secs = config.server.tcp_keepalive_secs,
        http1_keep_alive = config.server.http1_keep_alive,
        http1_header_read_timeout_secs = config.server.http1_header_read_timeout_secs,
        compression_enabled = config.server.compression_enabled,
        "Gateway HTTP transport settings"
    );
    if config.server.compression_enabled {
        tracing::info!("Response compression enabled");
        CompressionLayer::new()
    } else {
        tracing::info!("Response compression disabled (proxy disconnect governance)");
        CompressionLayer::new().no_gzip()
    }
}

async fn load_routes_and_warmup(
    db: &PgPool,
    tarpc_pool: &TarpcClientPool,
    upstream_http2_pool: &UpstreamHttp2ClientPool,
    config: &GatewayConfig,
) -> anyhow::Result<mazbri_routing::RouteEngine> {
    let route_engine = crate::route_source::load_route_engine(db).await?;
    if route_engine.is_empty() {
        match crate::route_source::check_tenants_count(db).await {
            Ok(0) => {
                tracing::info!(
                    "Gateway is in bootstrap mode (no tenants configured). Awaiting configuration."
                );
            }
            _ => {
                tracing::warn!(
                    "No active gateway route rules loaded from database; proxy requests will return no-route errors"
                );
            }
        }
    } else {
        tracing::info!(
            route_count = route_engine.route_count(),
            "Loaded gateway route rules from database"
        );
    }
    if let Err(error) = warmup_tarpc_clients(tarpc_pool, &config.services).await {
        tracing::warn!(error = % error, "Tarpc client warmup failed");
    }
    if let Err(error) = warmup_route_upstream_clients(upstream_http2_pool, &route_engine).await {
        tracing::warn!(error = % error, "Route upstream client warmup failed");
    }
    if let Err(error) = warmup_bypass_client(upstream_http2_pool, &config.services).await {
        tracing::warn!(error = % error, "Bypass HTTP/2 client warmup failed");
    }
    Ok(route_engine)
}

fn init_cbpro_client(
    config: &GatewayConfig,
    tls_ca_cert: &str,
    client_cert: &mazbri_tls::ManagedCertificate,
    tls_config: &mazbri_tls::TlsConfig,
) -> Option<CBProClient> {
    let endpoint = config.services.cbpro.as_ref()?;
    CBProClient::new(
        endpoint.clone(),
        tls_ca_cert,
        client_cert,
        tls_config,
        config.tarpc.client_ttl_secs,
    )
    .inspect_err(|error| {
        tracing::warn!(
            error = % error, url = % endpoint.url(),
            "Failed to initialize CBPro client; fallback support disabled"
        );
    })
    .ok()
}

fn init_cbpro_ext_client(
    config: &GatewayConfig,
    tls_ca_cert: &str,
    client_cert: &mazbri_tls::ManagedCertificate,
    tls_config: &mazbri_tls::TlsConfig,
) -> Option<CBProExtClient> {
    let endpoint = config.services.cbpro_ext.as_ref()?;
    CBProExtClient::new(
        endpoint.clone(),
        tls_ca_cert,
        client_cert,
        tls_config,
        config.tarpc.client_ttl_secs,
    )
    .inspect_err(|error| {
        tracing::warn!(
            error = % error, url = % endpoint.url(),
            "Failed to initialize CBPro-Ext client; runtime decision support disabled"
        );
    })
    .ok()
}

fn spawn_ratelimit_health_monitor(
    cache: crate::service_health_cache::ServiceHealthCache,
    tarpc_pool: TarpcClientPool,
    ratelimit_endpoint: crate::config::ServiceEndpoint,
) {
    spawn_supervised("service-health-monitor", async move {
        let mut last_state = ServiceHealthState::Unknown;
        let mut saw_unhealthy = false;
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            let current = cache.get("mazbri-ratelimit").await.state;
            if current == last_state {
                continue;
            }
            match current {
                ServiceHealthState::Unhealthy => {
                    saw_unhealthy = true;
                    tarpc_pool.remove_client(ratelimit_endpoint.url());
                    tracing::info!(
                        service = "mazbri-ratelimit",
                        "Removed cached tarpc client after health became unhealthy"
                    );
                }
                ServiceHealthState::Healthy if saw_unhealthy => {
                    tarpc_pool.remove_client(ratelimit_endpoint.url());
                    saw_unhealthy = false;
                    tracing::info!(
                        service = "mazbri-ratelimit",
                        "Removed cached tarpc client after health recovered"
                    );
                }
                ServiceHealthState::Healthy | ServiceHealthState::Unknown => {}
            }
            last_state = current;
        }
    });
}

fn spawn_background_workers(
    config: &GatewayConfig,
    db: &PgPool,
    route_engine: &Arc<arc_swap::ArcSwap<mazbri_routing::RouteEngine>>,
    admin_state: Option<&Arc<AdminState>>,
    proxy_state: &ProxyState,
) -> Arc<crate::route_reload::RouteReloadStatus> {
    spawn_supervised("callback-worker", {
        let cb_cfg = config.callback.to_runtime();
        let worker = CallbackWorker::with_config(db.clone(), &cb_cfg);
        async move {
            worker.run().await;
        }
    });
    tracing::info!(
        worker_count = config.callback.worker_count,
        batch_size = config.callback.batch_size,
        max_concurrent = config.callback.max_concurrent_deliveries,
        "CallbackWorker fleet background task started"
    );
    let route_reload_status = crate::route_reload::RouteReloadStatus::new(&config.route_reload);
    crate::route_reload::spawn_route_hot_reload(
        db.clone(),
        config.database.clone(),
        route_engine.clone(),
        config.route_reload.clone(),
        route_reload_status.clone(),
    );
    if let Some(admin) = admin_state {
        admin.set_routes_status_provider({
            let status = route_reload_status.clone();
            move || {
                serde_json::to_value(status.snapshot())
                    .unwrap_or_else(|_| serde_json::json!({ "error": "serialize_failed" }))
            }
        });
    }
    tracing::info!(
        interval_secs = config.route_reload.interval_secs,
        listen_notify = config.route_reload.listen_notify,
        "Route hot-reload background task started"
    );
    let product_service_for_watch = proxy_state.product_service.clone();
    let circuit_breakers_for_watch = proxy_state.circuit_breakers.clone();
    mazbri_core::config_watch::spawn_config_watch(
        db.clone(),
        config.database.clone(),
        config.config_watch.clone(),
        mazbri_core::config_watch::PRODUCT_DETAIL_VERSION_KEY,
        "gateway-product-detail",
        move || {
            let product_service = product_service_for_watch.clone();
            let circuit_breakers = circuit_breakers_for_watch.clone();
            async move {
                product_service.clear_cache();
                circuit_breakers.invalidate_all();
                if let Err(error) = product_service.warm_tenant_product_detail_cache().await {
                    tracing::warn!(
                        error = %error,
                        "Product detail cache warm after config watch failed; next request reads the database"
                    );
                }
            }
        },
    );
    tracing::info!(
        interval_secs = config.config_watch.interval_secs,
        listen_notify = config.config_watch.listen_notify,
        "Product detail config watch started"
    );
    route_reload_status
}

fn build_balance_cache(
    config: &GatewayConfig,
    redis: &mazbri_core::redis::RedisClient,
) -> Option<SharedBalancePrecheckCache> {
    if config.billing.enabled
        && config.billing.balance_check.enabled
        && config.billing.balance_check.cache.enabled
    {
        let cache_cfg = BalanceCacheConfig {
            enabled: true,
            l1_ttl: Duration::from_millis(config.billing.balance_check.cache.l1_ttl_ms.max(1)),
            l2_ttl_secs: config.billing.balance_check.cache.l2_ttl_secs.max(1),
            l1_max_capacity: 50_000,
        };
        tracing::info!(
            l1_ttl_ms = mazbri_core::num::millis_u64(cache_cfg.l1_ttl),
            l2_ttl_secs = cache_cfg.l2_ttl_secs,
            "Balance precheck cache enabled (L1 memory + L2 Redis)"
        );
        Some(Arc::new(BalancePrecheckCache::new(
            redis.clone(),
            &cache_cfg,
        )))
    } else {
        tracing::info!("Balance precheck cache disabled");
        None
    }
}

fn build_protected_routes(app_state: &AppState, config: &GatewayConfig) -> Router<AppState> {
    if app_state.billing_state.is_some() {
        let balance_check_middleware = if config.billing.balance_check.enabled {
            Some(axum::middleware::from_fn_with_state(
                app_state.clone(),
                crate::gateway::balance_check_middleware,
            ))
        } else {
            None
        };
        let ratelimit_middleware = axum::middleware::from_fn_with_state(
            app_state.clone(),
            crate::auth::ratelimit_middleware,
        );
        let auth_middleware = axum::middleware::from_fn_with_state(
            app_state.clone(),
            crate::auth::jwt_auth_middleware,
        );
        let mut proxy_route = post(unified_gateway_handler);
        if let Some(middleware) = balance_check_middleware {
            proxy_route = proxy_route.route_layer(middleware);
        }
        proxy_route = proxy_route.route_layer(ratelimit_middleware);
        proxy_route = proxy_route.route_layer(auth_middleware);
        let router = Router::new().route("/api/v1/gateway", proxy_route);
        tracing::info!(
            "Billing hooks enabled: balance_check={}, proxy_usage_record={}",
            config.billing.balance_check.enabled,
            config.billing.usage_record_enabled
        );
        router
    } else {
        Router::new()
            .route("/api/v1/gateway", post(unified_gateway_handler))
            .route_layer(axum::middleware::from_fn_with_state(
                app_state.clone(),
                crate::auth::ratelimit_middleware,
            ))
            .route_layer(axum::middleware::from_fn_with_state(
                app_state.clone(),
                crate::auth::jwt_auth_middleware,
            ))
    }
}

async fn create_billing_state(
    config: &GatewayConfig,
    db: &Arc<PgPool>,
) -> Option<BillingMiddlewareState> {
    if !config.billing.enabled {
        tracing::info!("Billing middleware disabled by configuration");
        return None;
    }
    let billing_db: Arc<PgPool> = if let Some(ref billing_config) = config.billing_database {
        match mazbri_core::create_pool(billing_config, None).await {
            Ok(pool) => {
                tracing::info!("Connected to billing database");
                pool
            }
            Err(e) => {
                tracing::error!("Failed to connect to billing database: {}", e);
                return None;
            }
        }
    } else {
        tracing::info!("Using main database for billing");
        Arc::clone(db)
    };
    let billing_service = Arc::new(BillingService::with_pgpool(
        Arc::clone(&billing_db),
        BillingServiceConfig::default(),
    ));
    let charge_queue = Arc::new(ChargeQueue::new((*billing_db).clone()));
    Some(BillingMiddlewareState::new(
        billing_service,
        Some(charge_queue),
    ))
}
fn build_cors_layer(config: &crate::config::CorsConfig) -> CorsLayer {
    let methods = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::PATCH,
        Method::OPTIONS,
    ];
    let headers = [
        header::AUTHORIZATION,
        header::CONTENT_TYPE,
        header::ACCEPT,
        header::HeaderName::from_static("x-request-id"),
        header::HeaderName::from_static("x-trace-id"),
        header::HeaderName::from_static("x-span-id"),
        header::HeaderName::from_static("x-parent-span-id"),
        header::HeaderName::from_static("traceparent"),
        header::HeaderName::from_static("x-mazbri-timestamp"),
        header::HeaderName::from_static("x-mazbri-nonce"),
        header::HeaderName::from_static("x-mazbri-signature"),
        header::HeaderName::from_static("x-mazbri-signature-input"),
        header::HeaderName::from_static("x-mazbri-key-id"),
    ];

    // Wildcard is allowed when credentials are off (enforced in CorsConfig::validate).
    // Never use CorsLayer::permissive() (it is overly open).
    if config.has_wildcard() {
        tracing::info!("CORS wildcard origin enabled; credentials disabled.");
        return CorsLayer::new()
            .allow_origin(AllowOrigin::any())
            .allow_methods(methods)
            .allow_headers(headers);
    }

    let mut origins = Vec::new();
    let mut invalid_origins = Vec::new();
    for origin in &config.allowed_origins {
        match origin.parse() {
            Ok(parsed) => origins.push(parsed),
            Err(e) => {
                invalid_origins.push(format!("'{origin}': {e}"));
            }
        }
    }
    if !invalid_origins.is_empty() {
        tracing::error!(
            invalid_origins = %invalid_origins.join(", "),
            "Invalid CORS origins detected; this indicates a configuration validation bug"
        );
    }
    if origins.is_empty() {
        // Safe default: no browser cross-origin access (typical for server-to-server tenants).
        tracing::info!("CORS: no allowed_origins configured; cross-origin browser access denied");
        return CorsLayer::new();
    }
    tracing::info!(
        origin_count = origins.len(),
        origins = %config.allowed_origins.join(", "),
        allow_credentials = config.allow_credentials,
        "CORS configured with allowed origins"
    );
    let mut cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods(methods)
        .allow_headers(headers);
    if config.allow_credentials {
        cors = cors.allow_credentials(true);
    }
    cors
}
async fn warmup_tarpc_clients(
    tarpc_pool: &TarpcClientPool,
    services: &crate::config::ServicesConfig,
) -> anyhow::Result<()> {
    warmup_tarpc_client_with_retry(
        tarpc_pool,
        mazbri_core::ServiceName::Ratelimit.as_str(),
        services.ratelimit.url(),
        || async {
            let _ = tarpc_pool
                .get_ratelimit_client(services.ratelimit.url())
                .await?;
            Ok(())
        },
    )
    .await?;
    warmup_tarpc_client_with_retry(
        tarpc_pool,
        mazbri_core::ServiceName::Auth.as_str(),
        services.auth.url(),
        || async {
            let _ = tarpc_pool.get_auth_client(services.auth.url()).await?;
            Ok(())
        },
    )
    .await?;
    warmup_tarpc_client_with_retry(
        tarpc_pool,
        mazbri_core::ServiceName::Billing.as_str(),
        services.billing.url(),
        || async {
            let _ = tarpc_pool
                .get_billing_client(services.billing.url())
                .await?;
            Ok(())
        },
    )
    .await?;
    warmup_tarpc_client_with_retry(
        tarpc_pool,
        mazbri_core::ServiceName::Cache.as_str(),
        services.cache.url(),
        || async {
            let _ = tarpc_pool.get_cache_client(services.cache.url()).await?;
            Ok(())
        },
    )
    .await?;
    if let Some(cbpro_url) = services.cbpro.as_ref() {
        warmup_tarpc_client_with_retry(
            tarpc_pool,
            mazbri_core::ServiceName::Cbpro.as_str(),
            cbpro_url.url(),
            || async {
                let _ = tarpc_pool.get_http_upstream_client(cbpro_url.url()).await?;
                Ok(())
            },
        )
        .await?;
    }
    tracing::info!("Tarpc clients warmed up");
    Ok(())
}
async fn warmup_route_upstream_clients(
    upstream_http2_pool: &UpstreamHttp2ClientPool,
    route_engine: &mazbri_routing::RouteEngine,
) -> anyhow::Result<()> {
    for service_url in route_engine.unique_service_urls() {
        warmup_upstream_http2_client_with_retry(
            upstream_http2_pool,
            "route-upstream",
            &service_url,
        )
        .await?;
    }
    tracing::info!(
        route_count = route_engine.route_count(),
        "Route upstream HTTP/2 clients warmed up"
    );
    Ok(())
}

/// transparent 出站走 bypass：预热与 execute 相同的 endpoint 形式，避免首包冷握手。
async fn warmup_bypass_client(
    upstream_http2_pool: &UpstreamHttp2ClientPool,
    services: &crate::config::ServicesConfig,
) -> anyhow::Result<()> {
    let Some(ref bypass) = services.bypass else {
        tracing::info!("No bypass_url configured; skip bypass HTTP/2 warmup");
        return Ok(());
    };
    let host_port = bypass.url();
    let endpoint = format!("https://{host_port}");
    warmup_upstream_http2_client_with_retry(
        upstream_http2_pool,
        mazbri_core::ServiceName::Bypass.as_str(),
        &endpoint,
    )
    .await?;
    tracing::info!(endpoint = %endpoint, "Bypass HTTP/2 client warmed up");
    Ok(())
}
async fn warmup_tarpc_client_with_retry<F, Fut>(
    _tarpc_pool: &TarpcClientPool,
    service_label: &str,
    addr: &str,
    warmup: F,
) -> anyhow::Result<()>
where
    F: Fn() -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    const MAX_RETRIES: u32 = 2;
    const INITIAL_DELAY_MS: u64 = 150;
    let mut attempt: u32 = 0;
    loop {
        match warmup().await {
            Ok(()) => return Ok(()),
            Err(error) => {
                let transient = error
                    .downcast_ref::<CoreError>()
                    .is_some_and(is_transient_warmup_error);
                if attempt < MAX_RETRIES && transient {
                    let delay_ms = INITIAL_DELAY_MS * 2u64.pow(attempt);
                    tracing::warn!(
                        service = % service_label,
                        addr = % addr,
                        attempt = attempt + 1,
                        max_retries = MAX_RETRIES,
                        delay_ms,
                        error = % error,
                        "Tarpc client warmup failed; retrying"
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    attempt += 1;
                    continue;
                }
                return Err(anyhow::anyhow!(
                    "Tarpc client warmup failed for {service_label} at {addr}: {error}"
                ));
            }
        }
    }
}
async fn warmup_upstream_http2_client_with_retry(
    upstream_http2_pool: &UpstreamHttp2ClientPool,
    service_label: &str,
    addr: &str,
) -> anyhow::Result<()> {
    const MAX_RETRIES: u32 = 2;
    const INITIAL_DELAY_MS: u64 = 150;
    let mut attempt: u32 = 0;
    loop {
        match upstream_http2_pool.warmup_endpoint(addr).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                if attempt < MAX_RETRIES && is_transient_warmup_error(&error) {
                    let delay_ms = INITIAL_DELAY_MS * 2u64.pow(attempt);
                    tracing::warn!(
                        service = % service_label,
                        addr = % addr,
                        attempt = attempt + 1,
                        max_retries = MAX_RETRIES,
                        delay_ms,
                        error = % error,
                        "Upstream HTTP/2 client warmup failed; retrying"
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    attempt += 1;
                    continue;
                }
                return Err(anyhow::anyhow!(
                    "Upstream HTTP/2 client warmup failed for {service_label} at {addr}: {error}"
                ));
            }
        }
    }
}
fn is_transient_warmup_error(error: &CoreError) -> bool {
    match error {
        CoreError::Io(io_error) => matches!(
            io_error.kind(),
            ErrorKind::ConnectionRefused
                | ErrorKind::ConnectionReset
                | ErrorKind::BrokenPipe
                | ErrorKind::NotConnected
                | ErrorKind::TimedOut
                | ErrorKind::WouldBlock
                | ErrorKind::Interrupted
                | ErrorKind::UnexpectedEof
        ),
        CoreError::Network(_) | CoreError::ServiceUnavailable { .. } => true,
        CoreError::Tls(message) => {
            let message = message.to_ascii_lowercase();
            message.contains("connection refused")
                || message.contains("connection reset")
                || message.contains("broken pipe")
                || message.contains("timed out")
                || message.contains("temporary failure")
                || message.contains("i/o timeout")
        }
        _ => false,
    }
}

fn spawn_supervised<F>(name: &'static str, fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    use futures::FutureExt;
    use std::panic::AssertUnwindSafe;

    tokio::spawn(async move {
        let result = AssertUnwindSafe(fut).catch_unwind().await;
        match result {
            Ok(()) => {
                tracing::warn!(
                    task = name,
                    "Background task exited unexpectedly (non-panic)."
                );
            }
            Err(panic_payload) => {
                let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic payload".to_string()
                };
                tracing::error!(
                    task = name,
                    panic = %msg,
                    "CRITICAL: Background task panicked and exited."
                );
            }
        }
    });
}

/// Reject over-capacity with 503 JSON + Retry-After (never silent RST).
async fn concurrency_limit_middleware(
    axum::extract::State(limit): axum::extract::State<Arc<Semaphore>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let permit: OwnedSemaphorePermit = if let Ok(permit) = limit.clone().try_acquire_owned() {
        permit
    } else {
        GATEWAY_OVERLOAD_REJECT_TOTAL.inc();
        tracing::warn!(
            target: "gateway.stage",
            event = "overload_reject",
            path = %req.uri().path(),
            message = "Request rejected: max_connections reached"
        );
        return ProxyError::ServiceUnavailable(
            "Service temporarily overloaded; retry later".to_string(),
        )
        .into_response();
    };
    let response = next.run(req).await;
    drop(permit);
    response
}

/// Convert empty / non-JSON 4xx·5xx bodies into gateway JSON envelope.
///
/// Covers `RequestBodyLimit` 413, hyper/timeout 空 body 500/504 等，避免客户端
/// 只看到 status 却解析不出 error 字段（G-16 收尾）。
async fn normalize_limit_errors_middleware(req: Request<Body>, next: Next) -> Response {
    // Prefer already-normalized id from request_id_middleware (inner layer may
    // not have run yet depending on stack order); always coerce UUID shape.
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(mazbri_core::normalize_request_id_uuid);
    let response = next.run(req).await;
    let status = response.status();
    if status == StatusCode::PAYLOAD_TOO_LARGE {
        return ProxyError::PayloadTooLarge("Request body exceeds size limit".to_string())
            .into_response();
    }
    if !status.is_client_error() && !status.is_server_error() {
        return response;
    }
    // 已有 JSON 错误体则放行
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let content_length = response
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    if content_type.contains("application/json") && content_length.is_some_and(|n| n > 0) {
        return response;
    }
    let (parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(b) => b,
        Err(_) => axum::body::Bytes::new(),
    };
    // 非空且看起来是 JSON → 原样回写
    if !bytes.is_empty() {
        let looks_json = bytes.first() == Some(&b'{') || bytes.first() == Some(&b'[');
        if looks_json || content_type.contains("application/json") {
            return Response::from_parts(parts, Body::from(bytes));
        }
    }
    // 空 body 或非 JSON 错误 → 统一 envelope
    let (code, message) = match status.as_u16() {
        504 => ("E5040", "Gateway Timeout"),
        502 => ("E5020", "Upstream service unavailable"),
        503 => ("E5030", "Service temporarily unavailable"),
        408 => ("E5040", "Request Timeout"),
        413 => ("E4130", "Request body exceeds size limit"),
        401 => ("E4010", "Unauthorized"),
        403 => ("E4030", "Forbidden"),
        404 => ("E4040", "Not Found"),
        429 => ("E4290", "Too Many Requests"),
        400..=499 => ("E4000", "Bad Request"),
        _ => ("E5000", "Internal Server Error"),
    };
    tracing::warn!(
        target: "gateway.stage",
        event = "empty_error_body_normalized",
        status = status.as_u16(),
        error_code = code,
        request_id = request_id.as_deref().unwrap_or(""),
        original_body_len = bytes.len(),
        message = "Replaced empty/non-JSON error body with gateway envelope"
    );
    crate::gateway::response::gateway_error_response(status, code, message, request_id)
}

fn handle_panic(err: Box<dyn std::any::Any + Send + 'static>) -> Response {
    GATEWAY_PANIC_TOTAL.inc();
    let detail = match err.downcast::<String>() {
        Ok(s) => *s,
        Err(other) => other
            .downcast_ref::<&str>()
            .map_or_else(|| "unknown panic".to_string(), |s| (*s).to_string()),
    };
    tracing::error!(
        target: "gateway.stage",
        event = "panic",
        panic = %detail,
        message = "Request handler panicked; returning 500 JSON"
    );
    ProxyError::InternalError("Internal Server Error".to_string()).into_response()
}

/// Connects the L1 usage-event producer when `billing.usage_events.sink = "redpanda"`.
/// A connect failure is logged and the gateway keeps the outbox path (never fails startup).
async fn build_usage_event_producer(
    config: &GatewayConfig,
) -> Option<Arc<mazbri_billing::UsageEventProducer>> {
    let cfg = &config.billing.usage_events;
    if !config.billing.enabled || !cfg.redpanda_enabled() {
        tracing::info!("Usage event sink: outbox (redpanda disabled)");
        return None;
    }
    match mazbri_billing::UsageEventProducer::connect(cfg).await {
        Ok(producer) => {
            tracing::info!(topic = %cfg.topic, brokers = ?cfg.brokers, "Usage event sink: redpanda");
            Some(Arc::new(producer))
        }
        Err(error) => {
            tracing::error!(error = %error, "Usage event producer connect failed; falling back to outbox path");
            None
        }
    }
}
