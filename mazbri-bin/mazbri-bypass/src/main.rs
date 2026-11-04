mod config;
mod metrics;
mod mgmt_probe;
mod server;
mod service;
mod ssrf;

use crate::config::BypassServiceConfig;
use crate::metrics::register_upstream_metrics;
use crate::service::MazbriBypassServiceImpl;
use crate::ssrf::SsrfChecker;
use anyhow::Result;
use mazbri_core::create_pool;
use mazbri_service::{init_service_runtime, run_toml_config_service};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    mazbri_core::metrics::init_service_metrics("mazbri-bypass");
    run_toml_config_service::<BypassServiceConfig, _, _>(|ctx| async move {
        let runtime = init_service_runtime(ctx)?;
        tracing::info!("Mazbri mazbri-bypass transparent egress gateway starting...");
        tracing::info!("Server configuration: {:?}", runtime.ctx.config.server);

        register_upstream_metrics()
            .map_err(|e| anyhow::anyhow!("Failed to register upstream metrics: {e}"))?;

        let db = create_pool(&runtime.ctx.config.database, None).await?;
        tracing::info!("Connected to platform database for upstream_artifacts");

        let admin_state = runtime.start_admin(&runtime.ctx.config.admin, Some(db.clone()))?;

        // `[ssrf].allow_private_network`（默认 false）：单机部署经前置代理/网关转内网时开启。
        runtime.ctx.config.ssrf.log_startup("mazbri-bypass");
        let ssrf_checker: SsrfChecker = runtime.ctx.config.ssrf.checker();
        // Connect-time DNS filter closes most SSRF rebinding windows (TOCTOU).
        let client = ssrf_checker
            .configure_http_client(
                reqwest::Client::builder()
                    .timeout(runtime.ctx.config.upstream.timeout())
                    .connect_timeout(runtime.ctx.config.upstream.connect_timeout())
                    .user_agent(&runtime.ctx.config.upstream.user_agent)
                    .pool_max_idle_per_host(32)
                    .pool_idle_timeout(std::time::Duration::from_secs(90))
                    .tcp_keepalive(std::time::Duration::from_secs(30)),
            )
            .build()?;
        let service = MazbriBypassServiceImpl::new(
            Arc::new(client),
            ssrf_checker,
            runtime.service_name.as_str().to_owned(),
            db,
        )
        .with_limits(mazbri_upstream_runtime::RuntimeLimits {
            ws_handshake_timeout: std::time::Duration::from_secs(
                runtime.ctx.config.upstream.ws_handshake_timeout_secs,
            ),
            ws_max_concurrent: runtime.ctx.config.upstream.ws_max_concurrent,
            ws_max_concurrent_per_key: runtime.ctx.config.upstream.ws_max_concurrent_per_key,
            ws_pool_max_idle_per_key: runtime.ctx.config.upstream.ws_pool_max_idle_per_key,
            ws_pool_idle_ttl: std::time::Duration::from_secs(
                runtime.ctx.config.upstream.ws_pool_idle_ttl_secs,
            ),
        });

        let addr = runtime.ctx.config.server.socket_addr()?;
        let grpc_service = service.clone();
        let cert = runtime.ctx.managed_certificate.clone();
        let max_connections = runtime.ctx.config.server.max_connections;
        let grpc_future = server::serve(addr, grpc_service, cert, max_connections);

        let probe_cfg = runtime.ctx.config.mgmt_probe.clone();
        let server_future = async move {
            if probe_cfg.enabled {
                let probe_service = service.clone();
                tokio::select! {
                    r = grpc_future => r,
                    r = mgmt_probe::serve(&probe_cfg, probe_service) => r,
                }
            } else {
                tracing::info!("mgmt_probe disabled; only data-plane mTLS :{}", addr.port());
                grpc_future.await
            }
        };

        Ok(runtime.into_output(admin_state, server_future))
    })
    .await
}
