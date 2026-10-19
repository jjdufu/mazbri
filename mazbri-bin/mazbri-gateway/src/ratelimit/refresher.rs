use super::engine::LocalRateLimitEngine;
use super::usage_buffer::UsageDeltaRecord;
use crate::config::ServiceEndpoint;
use crate::tarpc_client::TarpcClientPool;
use mazbri_rpc::ratelimit::{
    AckControlCommandsRequest, GetControlCommandsRequest, GetPolicySnapshotsRequest,
    IngestUsageDeltasRequest,
};
use std::sync::Arc;
use std::time::Duration;
use tarpc::context;
use tracing::{info, warn};

pub async fn refresh_snapshots_once(
    engine: Arc<LocalRateLimitEngine>,
    tarpc_pool: &TarpcClientPool,
    ratelimit_endpoint: &ServiceEndpoint,
) -> Result<(), String> {
    let client = tarpc_pool
        .get_ratelimit_client(ratelimit_endpoint.url(), ratelimit_endpoint.server_name())
        .await
        .map_err(|error| error.to_string())?;
    let request = GetPolicySnapshotsRequest {
        tenant_id: None,
        min_version: 0,
        include_disabled: true,
        page_size: 1000,
    };
    let response = match client
        .get_policy_snapshots(context::current(), request)
        .await
    {
        Ok(res) => res,
        Err(error) => {
            tarpc_pool.remove_client(ratelimit_endpoint.url()).await;
            return Err(error.to_string());
        }
    };
    if let Some(error) = response.error.as_ref() {
        return Err(error.message());
    }
    let snapshots = response
        .snapshots
        .into_iter()
        .filter_map(super::snapshot::TenantPolicySnapshot::from_rpc)
        .collect::<Vec<_>>();
    if !snapshots.is_empty() {
        engine.replace_snapshots(snapshots);
    } else {
        engine.update_snapshot_age_metric();
    }
    Ok(())
}

pub fn spawn_snapshot_refresher(
    engine: Arc<LocalRateLimitEngine>,
    tarpc_pool: TarpcClientPool,
    ratelimit_endpoint: ServiceEndpoint,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            match refresh_snapshots_once(Arc::clone(&engine), &tarpc_pool, &ratelimit_endpoint)
                .await
            {
                Ok(()) => {
                    super::metrics::LOCAL_RATELIMIT_REFRESH_TOTAL
                        .with_label_values(&["success"])
                        .inc();
                }
                Err(error) => {
                    super::metrics::LOCAL_RATELIMIT_REFRESH_TOTAL
                        .with_label_values(&["error"])
                        .inc();
                    engine.update_snapshot_age_metric();
                    warn!(error = % error, "Gateway ratelimit snapshot refresh failed");
                }
            }
        }
    })
}

pub fn spawn_usage_flusher(
    engine: Arc<LocalRateLimitEngine>,
    tarpc_pool: TarpcClientPool,
    ratelimit_endpoint: ServiceEndpoint,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let drained = engine.flush_usage_buffer();
            if drained.is_empty() {
                continue;
            }
            let deltas = drained
                .into_iter()
                .map(|UsageDeltaRecord { delta }| delta)
                .collect::<Vec<_>>();
            let client = match tarpc_pool
                .get_ratelimit_client(ratelimit_endpoint.url(), ratelimit_endpoint.server_name())
                .await
            {
                Ok(client) => client,
                Err(error) => {
                    warn!(error = % error, "Gateway ratelimit usage flush client acquisition failed");
                    super::metrics::LOCAL_RATELIMIT_USAGE_FLUSH_TOTAL
                        .with_label_values(&["error"])
                        .inc();
                    for delta in deltas {
                        engine.usage_buffer().record(delta);
                    }
                    continue;
                }
            };
            let request = IngestUsageDeltasRequest {
                deltas: deltas.clone(),
            };
            match client
                .ingest_usage_deltas(context::current(), request)
                .await
            {
                Ok(response) if response.success => {
                    engine.apply_usage_flush_result(response.ingested.max(0) as usize);
                }
                Ok(response) => {
                    super::metrics::LOCAL_RATELIMIT_USAGE_FLUSH_TOTAL
                        .with_label_values(&["error"])
                        .inc();
                    warn!(
                        error = ? response.error,
                        ingested = response.ingested,
                        "Gateway ratelimit usage flush rejected"
                    );
                }
                Err(error) => {
                    super::metrics::LOCAL_RATELIMIT_USAGE_FLUSH_TOTAL
                        .with_label_values(&["error"])
                        .inc();
                    warn!(error = % error, "Gateway ratelimit usage flush failed");
                    tarpc_pool.remove_client(ratelimit_endpoint.url()).await;
                    for delta in deltas {
                        engine.usage_buffer().record(delta);
                    }
                }
            }
        }
    })
}

pub fn spawn_command_poller(
    engine: Arc<LocalRateLimitEngine>,
    tarpc_pool: TarpcClientPool,
    ratelimit_endpoint: ServiceEndpoint,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            let client = match tarpc_pool
                .get_ratelimit_client(ratelimit_endpoint.url(), ratelimit_endpoint.server_name())
                .await
            {
                Ok(client) => client,
                Err(error) => {
                    warn!(error = % error, "Gateway ratelimit command poll client acquisition failed");
                    continue;
                }
            };
            match client
                .get_control_commands(
                    context::current(),
                    GetControlCommandsRequest {
                        tenant_id: None,
                        page_size: 1000,
                    },
                )
                .await
            {
                Ok(response) => {
                    if let Some(error) = response.error.as_ref() {
                        warn!(error = % error.message(), "Gateway ratelimit command poll rejected");
                        continue;
                    }
                    engine.update_control_commands(&response.commands);
                    let command_ids = response
                        .commands
                        .iter()
                        .map(|command| command.id.clone())
                        .collect::<Vec<_>>();

                    // refresh_policy: trigger snapshot refresh immediately (not only 5s cycle).
                    if engine.take_policy_refresh_request() {
                        match refresh_snapshots_once(
                            Arc::clone(&engine),
                            &tarpc_pool,
                            &ratelimit_endpoint,
                        )
                        .await
                        {
                            Ok(()) => {
                                super::metrics::LOCAL_RATELIMIT_REFRESH_TOTAL
                                    .with_label_values(&["success"])
                                    .inc();
                                info!(
                                    "Gateway ratelimit policy refresh triggered by control command"
                                );
                            }
                            Err(error) => {
                                super::metrics::LOCAL_RATELIMIT_REFRESH_TOTAL
                                    .with_label_values(&["error"])
                                    .inc();
                                warn!(
                                    error = % error,
                                    "Gateway ratelimit policy refresh (control command) failed"
                                );
                            }
                        }
                    }

                    if !command_ids.is_empty() {
                        let _ = client
                            .ack_control_commands(
                                context::current(),
                                AckControlCommandsRequest {
                                    gateway_id: engine.gateway_id().to_string(),
                                    command_ids,
                                },
                            )
                            .await;
                    }
                }
                Err(error) => {
                    warn!(error = % error, "Gateway ratelimit command poll failed");
                    tarpc_pool.remove_client(ratelimit_endpoint.url()).await;
                }
            }
        }
    })
}

pub fn spawn_bucket_janitor(engine: Arc<LocalRateLimitEngine>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            engine.update_snapshot_age_metric();
            let removed = engine.prune_buckets();
            if removed > 0 {
                info!(removed, "Removed stale local ratelimit buckets");
            }
        }
    })
}
