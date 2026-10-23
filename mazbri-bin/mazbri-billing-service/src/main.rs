mod config;
mod server;
mod tarpc_service;
use crate::config::BillingServiceConfig;
use anyhow::Result;
use mazbri_core::create_pool;
use mazbri_service::{init_service_runtime, run_toml_config_service};
use std::net::SocketAddr;

fn assert_port_available(addr: SocketAddr, label: &str) -> Result<()> {
    std::net::TcpListener::bind(addr)
        .map(drop)
        .map_err(|e| anyhow::anyhow!("Failed to reserve {label} at {addr} before startup: {e}"))
}

#[tokio::main]
async fn main() -> Result<()> {
    run_toml_config_service::<BillingServiceConfig, _, _>(|ctx| async move {
        let runtime = init_service_runtime(ctx)?;
        tracing::info!("Mazbri Billing Service starting...");
        tracing::info!("Configuration: {:?}", runtime.ctx.config.server);
        let server_addr = runtime.ctx.config.server.socket_addr()?;
        assert_port_available(server_addr, "billing tarpc server")?;
        if runtime.ctx.config.admin.enabled {
            let admin_addr = runtime.ctx.config.admin.socket_addr()?;
            assert_port_available(admin_addr, "billing admin server")?;
        }
        let db = create_pool(&runtime.ctx.config.database, None).await?;
        tracing::info!(
            max_connections = runtime.ctx.config.database.max_connections,
            acquire = ?runtime.ctx.config.database.acquire_timeout(),
            "Connected to billing write database"
        );
        let db_read = if let Some(ref read_cfg) = runtime.ctx.config.database_read {
            let pool = create_pool(read_cfg, None).await?;
            tracing::info!(
                max_connections = read_cfg.max_connections,
                acquire = ?read_cfg.acquire_timeout(),
                "Connected to billing read database (dual-pool)"
            );
            pool
        } else {
            tracing::warn!("database_read not configured; balance precheck shares the write pool");
            db.clone()
        };
        let admin_state = runtime.start_admin(&runtime.ctx.config.admin, Some(db.clone()))?;
        mazbri_billing::register_billing_metrics()
            .map_err(|e| anyhow::anyhow!("Failed to register billing metrics: {e}"))?;
        tracing::info!("计费指标已注册");
        let addr = runtime.ctx.config.server.socket_addr()?;
        tracing::info!("tarpc server listening on {} (mTLS mandatory)", addr);
        let worker_config = runtime.ctx.config.worker.clone();
        let server_future = server::serve(
            addr,
            db,
            db_read,
            runtime.service_name.as_str().to_owned(),
            Some(runtime.ctx.tls_acceptor.clone()),
            server::Durability {
                worker: worker_config,
                usage_events: runtime.ctx.config.usage_events.clone(),
                reconcile: runtime.ctx.config.reconcile.clone(),
            },
            runtime.ctx.config.server.max_connections,
        );
        Ok(runtime.into_output(admin_state, server_future))
    })
    .await
}
