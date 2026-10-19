mod config;
mod metrics;
mod ssrf;
mod service;
mod server;

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
    run_toml_config_service::<BypassServiceConfig, _, _>(
        "mazbri-bypass",
        "mazbri-bypass",
        |ctx| async move {
            let runtime = init_service_runtime(ctx, "mazbri-bypass")?;
            tracing::info!("Mazbri mazbri-bypass transparent egress gateway starting...");
            tracing::info!("Server configuration: {:?}", runtime.ctx.config.server);

            register_upstream_metrics()
                .map_err(|e| anyhow::anyhow!("Failed to register upstream metrics: {}", e))?;

            let db = create_pool(&runtime.ctx.config.database, None).await?;
            tracing::info!("Connected to platform database for upstream_artifacts");

            let admin_state =
                runtime.start_admin(&runtime.ctx.config.admin, Some(db.clone()))?;

            let ssrf_checker = SsrfChecker::new();
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
                runtime.service_name.clone(),
                db,
            )
            .with_limits(mazbri_upstream_runtime::RuntimeLimits {
                ws_handshake_timeout: std::time::Duration::from_secs(
                    runtime.ctx.config.upstream.ws_handshake_timeout_secs,
                ),
                job_poll_min_interval: std::time::Duration::from_millis(
                    runtime.ctx.config.upstream.job_poll_min_interval_ms,
                ),
            });

            let addr = runtime.ctx.config.server.socket_addr()?;
            let server_future = server::serve(
                addr,
                service,
                runtime.ctx.managed_certificate.clone(),
                runtime.ctx.config.server.max_connections,
            );

            Ok(runtime.into_output(admin_state, server_future))
        },
    )
    .await
}
