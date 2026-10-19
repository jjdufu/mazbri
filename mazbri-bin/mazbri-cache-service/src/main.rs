mod config;
mod server;
mod tarpc_service;
use crate::config::CacheServiceConfig;
use anyhow::Result;
use mazbri_core::create_pool;
use mazbri_service::{init_service_runtime, run_toml_config_service};
#[tokio::main]
async fn main() -> Result<()> {
    run_toml_config_service::<CacheServiceConfig, _, _>(
        "mazbri-cache-service",
        "cache-service",
        |ctx| async move {
            let runtime = init_service_runtime(ctx, "mazbri-cache-service")?;
            tracing::info!("Mazbri Cache Service starting...");
            tracing::info!("Configuration: {:?}", runtime.ctx.config.server);
            if let Err(e) = mazbri_cache::register_cache_metrics() {
                tracing::warn!("Failed to register cache metrics: {}", e);
            } else {
                tracing::info!("Cache metrics registered successfully");
            }
            let db = create_pool(&runtime.ctx.config.database, None).await?;
            tracing::info!("Connected to database");
            let admin_state = runtime.start_admin(&runtime.ctx.config.admin, Some(db.clone()))?;
            let cache_config = mazbri_cache::CacheConfig {
                default_ttl: runtime.ctx.config.cache.default_ttl,
                max_cache_size: runtime.ctx.config.cache.max_cache_size,
                max_entry_size: 1024 * 1024,
                api_ttl: runtime.ctx.config.cache.api_ttl.clone(),
                tenant_isolated: runtime.ctx.config.cache.tenant_isolated,
            };
            let addr = runtime.ctx.config.server.socket_addr()?;
            let redis = mazbri_core::redis::RedisClient::connect_with_config(&runtime.ctx.config.redis)
                .await
                .map_err(|error| anyhow::anyhow!("Failed to connect to Redis: {}", error))?;
            tracing::info!("tarpc server listening on {} (mTLS mandatory)", addr);
            let server_future = server::serve(
                addr,
                db,
                cache_config,
                redis,
                Some(runtime.ctx.tls_acceptor.clone()),
                runtime.ctx.config.server.max_connections,
            );
            Ok(runtime.into_output(admin_state, server_future))
        },
    )
    .await
}
