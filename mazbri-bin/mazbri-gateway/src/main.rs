mod auth;
mod balance_cache;
mod cbpro_client;
mod cbpro_ext_client;
mod circuit_breaker;
mod config;
mod crypto;
mod gateway;
mod health;
mod http2_upstream_client;
mod interaction_store;
mod openapi;
mod ratelimit;
mod redis_client;
mod registry_handler;
mod route_reload;
mod route_source;
mod router;
mod service_health_cache;
mod startup_metrics;
mod supplier_config_cache;
mod tarpc_client;
mod upstream_contract;
use crate::auth::{JwtKeyManager, register_auth_middleware_metrics};
use crate::config::GatewayConfig;
use crate::crypto::register_crypto_metrics;
use crate::gateway::register_proxy_metrics;
use crate::ratelimit::register_local_ratelimit_metrics;
use crate::startup_metrics::{register_gateway_startup_metrics, set_gateway_startup_ready};
use anyhow::Result;
use axum::serve::ListenerExt;
use mazbri_core::{
    config::{
        PureBootstrap, ServiceConfig, ValidateConfig, build_log_routes, get_cert_service_token,
        load_toml,
    },
    create_pool, init_tracing_with_routes, register_log_writer_metrics, register_metrics,
};
use mazbri_registry::{
    ConfigCenterRegistry, ConfigHandler, HttpConfigClientBuilder, ServiceRegistry,
};
use mazbri_service::{create_shutdown_manager, start_simple_admin_server};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;
#[tokio::main]
async fn main() -> Result<()> {
    let bootstrap_path = mazbri_core::config::resolve_bootstrap_path_mandatory()?;
    let bootstrap: PureBootstrap = load_toml(&bootstrap_path)?;
    let service = bootstrap.service_name.name();
    let service_name = service.as_str().to_owned();
    let log_routes = build_log_routes(&bootstrap.log_routes)?;
    let cert_service_token_value = get_cert_service_token()?;
    let tls_config = mazbri_tls::TlsConfig {
        cert_service_url: bootstrap.cert_service.base().to_owned(),
        service_identity: service_name.clone(),
        cert_service_token: cert_service_token_value,
        ..Default::default()
    };
    info!(
        "TLS 配置已从 bootstrap 加载: service_identity={}",
        tls_config.service_identity
    );
    let config = GatewayConfig::load_from_config_center(&bootstrap).await?;
    config.validate()?;
    let admin_state = start_simple_admin_server(&service_name, &config.admin)?;
    let _log_guard = init_tracing_with_routes(&bootstrap.tracing, &service_name, &log_routes)?;
    info!("Mazbri API Gateway starting...");
    info!("Configuration loaded from config center");
    info!("Server configuration: {:?}", config.server);
    register_all_metrics()?;
    info!("Initializing mTLS for Gateway...");
    let cert_manager = mazbri_tls::CertManager::new(tls_config.clone());
    let cert = cert_manager
        .initialize()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize TLS certificate: {e}"))?;
    let renewal_task = cert_manager.spawn_renewal_task_managed();
    info!(
        service_identity = % tls_config.service_identity,
        "mTLS enabled for gateway client connections"
    );
    info!(
        subject = % cert.subject, expires_at = % cert.expires_at,
        "TLS certificate initialized"
    );
    let tls_ca_cert = cert.ca_cert_pem.clone();
    info!("mTLS initialized successfully");
    let db = create_pool(&config.database, None).await?;
    info!("Connected to database");
    let jwt_manager = JwtKeyManager::new(Arc::clone(&db), "mazbri-gateway-v1");
    let jwt_secret = jwt_manager
        .get_jwt_secret()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to load JWT secret from database: {e}"))?;
    info!("JWT secret loaded from database (HKDF derived)");
    info!("Using log-based request logging (tracing output)");
    let service_health_cache = {
        info!("Service health cache enabled with dynamic registry monitoring (interval 15s)");
        let cache = service_health_cache::ServiceHealthCache::new();
        let cert_url = bootstrap.cert_service.base().to_owned();
        let token = tls_config.cert_service_token.clone();
        let _handle = cache.clone().spawn_dynamic(cert_url, token);
        Some(cache)
    };
    let app = router::create_router(router::RouterInit {
        service_name: service_name.clone(),
        config: config.clone(),
        db,
        tls_ca_cert,
        client_cert: cert,
        tls_config: &tls_config,
        jwt_secret,
        admin_state: admin_state.clone(),
        service_health_cache,
    })
    .await?;
    let _shutdown_handle = create_shutdown_manager();
    let registry = start_registry(&bootstrap, service, &service_name, &config).await?;
    let (addr, listener, configure) = bind_listener(&config.server).await?;
    let listener = listener.tap_io(configure);
    if let Some(ref state) = admin_state {
        state.set_ready(true);
        info!("Service marked as ready");
    }
    set_gateway_startup_ready(true);
    info!("Server listening on {}", addr);
    let registry_for_shutdown = registry.clone();
    let shutdown_timeout = config.server.graceful_shutdown_timeout();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(graceful_shutdown(
        renewal_task,
        registry_for_shutdown,
        shutdown_timeout,
    ))
    .await?;
    // Server no longer accepts or serves requests: flush post-response billing work.
    // Keep (graceful_shutdown_timeout + this) below the container stop grace period.
    gateway::post_response::drain_on_shutdown(USAGE_CHARGE_DRAIN_TIMEOUT).await;
    Ok(())
}

/// Upper bound for flushing deferred usage charges at shutdown.
const USAGE_CHARGE_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

async fn graceful_shutdown(
    renewal_task: mazbri_tls::RenewalTaskHandle,
    registry_for_shutdown: Option<Arc<ConfigCenterRegistry>>,
    shutdown_timeout: std::time::Duration,
) {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => { info!("Received Ctrl+C"); }
        () = mazbri_service::wait_for_termination_signal() => {
            info!("Received termination signal");
        }
    }
    renewal_task.shutdown().await;
    if let Some(ref reg) = registry_for_shutdown
        && let Err(e) = reg.shutdown().await
    {
        tracing::warn!("注册中心关闭失败: {}", e);
    }
    // Drain in-flight requests up to graceful_shutdown_timeout (axum already
    // stops accepting; sleep allows handlers to finish before process exit).
    info!(
        timeout_secs = shutdown_timeout.as_secs(),
        "Draining in-flight requests before exit"
    );
    tokio::time::sleep(shutdown_timeout).await;
    info!("Graceful shutdown complete");
}

fn register_all_metrics() -> Result<()> {
    register_metrics()?;
    register_crypto_metrics()?;
    register_proxy_metrics()?;
    register_auth_middleware_metrics()?;
    register_local_ratelimit_metrics()?;
    register_gateway_startup_metrics()?;
    crate::route_reload::register_route_reload_metrics()?;
    mazbri_auth::register_auth_metrics()?;
    register_log_writer_metrics()?;
    set_gateway_startup_ready(false);
    Ok(())
}

async fn start_registry(
    bootstrap: &PureBootstrap,
    service: mazbri_core::ServiceName,
    service_name: &str,
    config: &GatewayConfig,
) -> Result<Option<Arc<ConfigCenterRegistry>>> {
    let registry = if let Some(ref registration) = bootstrap.registration {
        if registration.enabled {
            let token = get_cert_service_token()?;
            let http_client = Arc::new(
                HttpConfigClientBuilder::new(service_name, bootstrap.cert_service.base(), token)
                    .build()
                    .await?,
            );
            let handler = registry_handler::create_gateway_config_handler();
            let metadata = registration.to_service_metadata(
                service,
                config.server.port,
                config.admin_health(),
            );
            let registry = Arc::new(ConfigCenterRegistry::new(
                http_client,
                handler as Arc<dyn ConfigHandler>,
                metadata,
            ));
            registry
                .register(&registry.metadata())
                .await
                .map_err(|e| anyhow::anyhow!("节点注册失败: {e}"))?;
            registry
                .start()
                .await
                .map_err(|e| anyhow::anyhow!("注册中心启动失败: {e}"))?;
            info!("节点注册成功，心跳已启动");
            Some(registry)
        } else {
            info!("注册中心未启用 (registration.enabled=false)");
            None
        }
    } else {
        None
    };
    Ok(registry)
}

/// 绑定监听地址，返回地址、监听器与「接入连接时」设置 NODELAY/keepalive 的回调。
async fn bind_listener(
    server: &mazbri_core::ServerConfig,
) -> Result<(
    SocketAddr,
    tokio::net::TcpListener,
    impl FnMut(&mut tokio::net::TcpStream) + Send + 'static,
)> {
    let addr = server.socket_addr()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let tcp_nodelay = server.tcp_nodelay;
    let tcp_keepalive = server.tcp_keepalive();
    let configure = move |stream: &mut tokio::net::TcpStream| {
        if tcp_nodelay && let Err(err) = stream.set_nodelay(true) {
            tracing::debug!(error = %err, "failed to set TCP_NODELAY on accepted connection");
        }
        if let Some(keepalive) = tcp_keepalive
            && let Err(err) = apply_tcp_keepalive(stream, keepalive)
        {
            tracing::debug!(error = %err, "failed to set TCP keepalive on accepted connection");
        }
    };
    info!(
        tcp_nodelay,
        tcp_keepalive_secs = server.tcp_keepalive_secs,
        http1_keep_alive = server.http1_keep_alive,
        http1_header_read_timeout_secs = server.http1_header_read_timeout_secs,
        max_connections = server.max_connections,
        "TCP accept options applied (NODELAY/keepalive); HTTP/1 keep-alive uses hyper defaults when keep_alive={}",
        server.http1_keep_alive
    );
    Ok((addr, listener, configure))
}

fn apply_tcp_keepalive(
    stream: &tokio::net::TcpStream,
    idle: std::time::Duration,
) -> std::io::Result<()> {
    use socket2::{SockRef, TcpKeepalive};
    let sock = SockRef::from(stream);
    // with_time is portable; interval requires broader platform support on socket2 0.5.
    let keepalive = TcpKeepalive::new().with_time(idle);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let keepalive = keepalive.with_interval(idle);
    sock.set_tcp_keepalive(&keepalive)
}
