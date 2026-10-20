mod config;
mod server;
mod tarpc_service;
use crate::config::AuthServiceConfig;
use anyhow::Result;
use mazbri_core::create_pool;
use mazbri_service::{init_service_runtime, run_toml_config_service};
#[tokio::main]
async fn main() -> Result<()> {
    run_toml_config_service::<AuthServiceConfig, _, _>(|ctx| async move {
            let runtime = init_service_runtime(ctx)?;
            tracing::info!("Mazbri Authentication Service starting...");
            tracing::info!("Server configuration: {:?}", runtime.ctx.config.server);
            let db = create_pool(&runtime.ctx.config.database, None).await?;
            tracing::info!("Connected to database");
            let platform =
                if let Some(platform_database) = runtime.ctx.config.platform_database.clone() {
                    Some(server::PlatformListen {
                        pool: create_pool(&platform_database, None).await?,
                        database: platform_database,
                    })
                } else {
                    None
                };
            let admin_state = runtime.start_admin(&runtime.ctx.config.admin, Some(db.clone()))?;
            let auth_config = mazbri_auth::AuthServiceConfig {
                api_key_prefix_length: runtime.ctx.config.auth.api_key_prefix_length,
                enable_ip_whitelist: runtime.ctx.config.auth.enable_ip_whitelist,
                validation_cache_ttl_secs: runtime.ctx.config.auth.validation_cache_ttl_secs,
                validation_cache_max_entries: runtime.ctx.config.auth.validation_cache_max_entries,
            };
            mazbri_auth::register_auth_metrics()
                .map_err(|e| anyhow::anyhow!("Failed to register auth metrics: {e}"))?;
            tracing::info!("认证指标已注册");
            let addr = runtime.ctx.config.server.socket_addr()?;
            tracing::info!("tarpc server listening on {} (mTLS mandatory)", addr);
            let server_future = server::serve(
                addr,
                db,
                auth_config,
                Some(runtime.ctx.tls_acceptor.clone()),
                runtime.ctx.config.server.max_connections,
                platform,
                runtime.ctx.config.config_watch.clone(),
            );
            Ok(runtime.into_output(admin_state, server_future))
        },
    )
    .await
}
