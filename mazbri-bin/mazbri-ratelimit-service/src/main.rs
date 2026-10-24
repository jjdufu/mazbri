mod config;
mod server;
mod tarpc_service;
use crate::config::RateLimitServiceConfig;
use anyhow::Result;
use mazbri_core::create_pool;
use mazbri_service::{init_service_runtime, run_toml_config_service};

#[tokio::main]
async fn main() -> Result<()> {
    run_toml_config_service::<RateLimitServiceConfig, _, _>(|ctx| async move {
        let runtime = init_service_runtime(ctx)?;
        tracing::info!("Mazbri Rate Limiting Service starting...");
        tracing::info!("Server configuration: {:?}", runtime.ctx.config.server);
        let db = create_pool(&runtime.ctx.config.database, None).await?;
        tracing::info!("Connected to database");
        let admin_state = runtime.start_admin(&runtime.ctx.config.admin, Some(db.clone()))?;
        let audit_enabled = runtime.ctx.config.is_audit_enabled();
        let addr = runtime.ctx.config.server.socket_addr()?;
        tracing::info!("tarpc server listening on {} (mTLS mandatory)", addr);
        let server_future = server::serve_with_config(
            addr,
            db,
            Some(runtime.ctx.tls_acceptor.clone()),
            audit_enabled,
            runtime.ctx.config.server.max_connections,
        );
        Ok(runtime.into_output(admin_state, server_future))
    })
    .await
}

/// Process-wide allocator: mimalloc. Under glibc malloc the service kept
/// ~107MiB anon after request bursts (spread over per-thread glibc arenas)
/// and only returned it sporadically; mimalloc returns freed segments to the
/// OS and matches the gateway allocator.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
