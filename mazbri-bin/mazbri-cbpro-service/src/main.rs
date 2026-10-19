mod cache;
mod config;
mod db;
mod manager;
mod server;
mod tarpc_service;
use crate::config::CBProServiceConfig;
use crate::db::RedisFallbackRepository;
use crate::manager::FallbackManager;
use crate::tarpc_service::CBProServiceImpl;
use anyhow::Result;
use mazbri_service::{init_service_runtime, run_toml_config_service};
use std::sync::Arc;
#[tokio::main]
async fn main() -> Result<()> {
    run_toml_config_service::<CBProServiceConfig, _, _>(
        "mazbri-cbpro-service",
        "cbpro-service",
        |ctx| async move {
            let runtime = init_service_runtime(ctx, "mazbri-cbpro-service")?;
            tracing::info!("Mazbri CBPro Service starting...");
            tracing::info!(
                "Server config: {}:{}",
                runtime.ctx.config.server.host,
                runtime.ctx.config.server.port
            );
            let redis = Arc::new(RedisFallbackRepository::new(
                mazbri_core::redis::RedisClient::connect_with_config(&runtime.ctx.config.redis)
                    .await
                    .map_err(|error| anyhow::anyhow!("Failed to connect to Redis: {}", error))?,
            ));
            tracing::info!("Redis connection established");
            let admin_state = runtime.start_admin(&runtime.ctx.config.admin, None)?;
            mazbri_core::metrics::register_cbpro_metrics()
                .map_err(|e| anyhow::anyhow!("Failed to register cbpro metrics: {}", e))?;
            tracing::info!("CBPro 指标已注册");
            let manager = Arc::new(FallbackManager::new(
                runtime.ctx.config.clone(),
                Some(redis),
            ));
            tracing::info!("Fallback manager initialized with hybrid cache (memory + redis)");
            let service = CBProServiceImpl::new(manager);
            let addr = runtime.ctx.config.server.socket_addr()?;
            tracing::info!(
                "tarpc server listening on {} (mTLS mandatory, max connections: {})",
                addr,
                runtime.ctx.config.server.max_connections
            );
            let server_future = server::serve(
                addr,
                service,
                Some(runtime.ctx.tls_acceptor.clone()),
                runtime.ctx.config.server.max_connections,
            );
            Ok(runtime.into_output(admin_state, server_future))
        },
    )
    .await
}
