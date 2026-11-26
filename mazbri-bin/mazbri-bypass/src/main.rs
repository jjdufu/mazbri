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
        // Connect-time DNS filter + disabled redirects + .no_proxy() closes SSRF & redirect leaks.
        let client = Arc::new(
            ssrf_checker
                .safe_http_client_builder(
                    runtime.ctx.config.upstream.connect_timeout(),
                    runtime.ctx.config.upstream.timeout(),
                )
                .user_agent(&runtime.ctx.config.upstream.user_agent)
                .pool_max_idle_per_host(
                    runtime
                        .ctx
                        .config
                        .upstream
                        .http_pool_max_idle_per_host
                        .clamp(1, 4096),
                )
                .pool_idle_timeout(std::time::Duration::from_secs(90))
                .tcp_nodelay(true)
                .tcp_keepalive(std::time::Duration::from_secs(30))
                .build()?,
        );
        let runtime_opts =
            mazbri_upstream_runtime::RuntimeOptions::from(&runtime.ctx.config.upstream);
        warm_and_spawn_root_material_keyring(&runtime.ctx.tls_config).await?;
        let grpc_service = MazbriBypassServiceImpl::new(
            Arc::clone(&client),
            ssrf_checker.clone(),
            runtime.service_name.as_str().to_owned(),
            Arc::clone(&db),
        )
        .with_http_client_defaults(
            &runtime.ctx.config.upstream.user_agent,
            runtime.ctx.config.upstream.connect_timeout(),
            runtime.ctx.config.upstream.timeout(),
        )
        .with_options(runtime_opts.clone());

        // Management probe (:8890) gets its own isolated RuntimeEngine (separate WsPool,
        // TokenCache, RetryBudgets, and config caches) so probe runs cannot starve data-plane traffic.
        let probe_service = MazbriBypassServiceImpl::new(
            client,
            ssrf_checker,
            runtime.service_name.as_str().to_owned(),
            db,
        )
        .with_http_client_defaults(
            &runtime.ctx.config.upstream.user_agent,
            runtime.ctx.config.upstream.connect_timeout(),
            runtime.ctx.config.upstream.timeout(),
        )
        .with_options(runtime_opts);

        let addr = runtime.ctx.config.server.socket_addr()?;
        let cert_handle = runtime.ctx.cert_handle.clone();
        let tls_config = runtime.ctx.tls_config.clone();
        let max_connections = runtime.ctx.config.server.max_connections;
        let grpc_future = server::serve(addr, grpc_service, cert_handle, tls_config, max_connections);

        let probe_cfg = runtime.ctx.config.mgmt_probe.clone();
        let server_future = async move {
            if probe_cfg.enabled {
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
    .await?;
    service::drain_pending_logs(std::time::Duration::from_secs(5)).await;
    Ok(())
}

async fn refresh_bypass_root_keyring(
    cert_client: &reqwest::Client,
    cert_url: &str,
    cert_token: &str,
) -> Result<()> {
    let protection = mazbri_core::crypto::tenant_key::fetch_root_protection_keyring(
        cert_client,
        cert_url,
        cert_token,
    )
    .await
    .map_err(|e| anyhow::anyhow!("fetch root protection keyring failed: {e}"))?;
    let keyring =
        mazbri_core::crypto::supplier_secrets::RootMaterialKeyring::from_protection_keyring(
            &protection,
        )
        .map_err(|e| anyhow::anyhow!("decode root material keyring failed: {e}"))?;
    mazbri_core::crypto::supplier_secrets::install_root_material_keyring(keyring);
    Ok(())
}

async fn warm_and_spawn_root_material_keyring(tls_config: &mazbri_tls::TlsConfig) -> Result<()> {
    let cert_client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    refresh_bypass_root_keyring(
        &cert_client,
        &tls_config.cert_service_url,
        &tls_config.cert_service_token,
    )
    .await?;
    tracing::info!("Pre-warmed multi-version commercial root material keyring for mazbri-bypass");
    let cert_url = tls_config.cert_service_url.clone();
    let cert_token = tls_config.cert_service_token.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(err) = refresh_bypass_root_keyring(&cert_client, &cert_url, &cert_token).await
            {
                tracing::warn!(error = %err, "Periodic bypass root material keyring refresh failed");
            }
        }
    });
    Ok(())
}
