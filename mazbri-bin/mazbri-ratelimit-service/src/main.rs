mod audit;
mod config;
mod server;
mod tarpc_service;
use crate::config::RateLimitServiceConfig;
use anyhow::Result;
use mazbri_core::create_pool;
use mazbri_service::{init_service_runtime, run_toml_config_service};

/// 停机时统一审计队列的最长排空时间。
const AUDIT_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

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
        let (audit_sink, audit_handle) = audit::start(&runtime.ctx.config.rate_limit.audit)?
            .map_or((None, None), |(sink, handle)| (Some(sink), Some(handle)));
        let addr = runtime.ctx.config.server.socket_addr()?;
        tracing::info!("tarpc server listening on {} (mTLS mandatory)", addr);
        let serve = server::serve_with_config(
            addr,
            db,
            Some(runtime.ctx.tls_acceptor.clone()),
            audit_enabled,
            audit_sink,
            runtime.ctx.config.server.max_connections,
        );
        let server_future = async move {
            let result = serve.await;
            if let Some(handle) = audit_handle {
                handle.shutdown(AUDIT_DRAIN_TIMEOUT).await;
            }
            result
        };
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
