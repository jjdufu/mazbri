mod cbpro_client;
mod config;
mod db;
mod greptime;
mod manager;
mod observations;
mod runtime;
mod server;
mod tarpc_service;
mod validation;

use crate::cbpro_client::CBProClient;
use crate::config::CBProExtServiceConfig;
use crate::db::CbProExtRepository;
use crate::greptime::GreptimeClient;
use crate::manager::CbProExtManager;
use crate::tarpc_service::CBProExtServiceImpl;
use anyhow::Result;
use mazbri_core::create_pool;
use mazbri_service::{init_service_runtime, run_toml_config_service};
use std::sync::Arc;

fn spawn_greptime_refresh(manager: &Arc<CbProExtManager>, config: &CBProExtServiceConfig) {
    if config.greptime.enabled {
        let manager = Arc::clone(manager);
        let interval_secs = config.greptime.query_interval_secs.max(5);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                if let Err(error) = manager.refresh_runtime_decisions_from_greptime().await {
                    tracing::warn!(error = % error, "Greptime refresh failed");
                }
            }
        });
    }
}

fn spawn_validation_worker(manager: &Arc<CbProExtManager>, config: &CBProExtServiceConfig) {
    if config.validation.enabled {
        let manager = Arc::clone(manager);
        let poll_interval = config.validation.job_poll_interval_ms.max(100);
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_millis(poll_interval));
            let mut missing_schema_logged = false;
            loop {
                interval.tick().await;
                if let Err(error) = manager.process_validation_jobs().await {
                    if crate::db::CbProExtRepository::is_missing_cbpro_schema_error(&error) {
                        if !missing_schema_logged {
                            tracing::info!(
                                error = % error,
                                "CBPro validation schema is not initialized yet; validation worker will stay idle"
                            );
                            missing_schema_logged = true;
                        }
                    } else {
                        tracing::warn!(error = % error, "Validation worker loop failed");
                    }
                } else if missing_schema_logged {
                    tracing::info!(
                        "CBPro validation schema became available; validation worker resumed"
                    );
                    missing_schema_logged = false;
                }
            }
        });
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    run_toml_config_service::<CBProExtServiceConfig, _, _>(|ctx| async move {
            let runtime = init_service_runtime(ctx)?;
            tracing::info!("Mazbri CBPro-Ext Service starting...");
            tracing::info!(
                "Server config: {}:{}",
                runtime.ctx.config.server.host,
                runtime.ctx.config.server.port
            );
            mazbri_core::register_metrics()
                .map_err(|e| anyhow::anyhow!("Failed to register metrics: {e}"))?;
            let platform_db = if let Some(ref platform_config) = runtime.ctx.config.platform_database {
                tracing::info!("Connecting to platform database...");
                Some(create_pool(platform_config, None).await?)
            } else {
                None
            };
            let admin_state = runtime.start_admin(&runtime.ctx.config.admin, platform_db.clone())?;
            let cert_manager = mazbri_tls::CertManager::new(runtime.ctx.tls_config.clone());
            let client_cert = cert_manager.initialize().await?;
            let cbpro_client = match CBProClient::new(
                runtime.ctx.config.clients.cbpro.clone(),
                &client_cert.ca_cert_pem,
                &client_cert,
                &runtime.ctx.tls_config,
            ) {
                Ok(client) => Some(Arc::new(client)),
                Err(error) => {
                    tracing::warn!(error = % error, "Failed to initialize CBPro client; validation support degraded");
                    None
                }
            };
            let greptime_client = GreptimeClient::new(&runtime.ctx.config.greptime)?;
            let repo = Arc::new(CbProExtRepository::new(platform_db.clone(), platform_db.clone()));
            let manager = Arc::new(CbProExtManager::new(
                runtime.ctx.config.clone(),
                repo,
                cbpro_client,
                greptime_client,
            ));
            if let Err(error) = manager.refresh_runtime_decisions_from_db().await {
                if crate::db::CbProExtRepository::is_missing_cbpro_schema_error(&error) {
                    tracing::info!(
                        error = % error,
                        "CBPro schema is not initialized yet; runtime decision restore skipped"
                    );
                } else {
                    tracing::warn!(error = % error, "Failed to restore runtime decisions from DB");
                }
            }
            manager.restore_runtime_decisions_from_platform();
            spawn_greptime_refresh(&manager, &runtime.ctx.config);
            spawn_validation_worker(&manager, &runtime.ctx.config);
            let service = CBProExtServiceImpl::new(manager);
            let addr = runtime.ctx.config.server.socket_addr()?;
            tracing::info!(
                "CBPro-Ext tarpc server listening on {} (mTLS mandatory, max connections: {})",
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
