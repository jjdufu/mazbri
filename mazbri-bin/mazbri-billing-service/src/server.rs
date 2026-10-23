use crate::config::WorkerConfig;
use crate::tarpc_service::BillingTarpcService;
use anyhow::Result;
use mazbri_billing::{
    BillingService, BillingServiceConfig, ChargeQueue, ChargeWorker, QueueConfig, UsageOutboxWorker,
};
use mazbri_rpc::billing::BillingService as BillingServiceTrait;
use mazbri_tls::TlsAcceptor;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

/// L1 consumer + L3 reconcile settings.
pub struct Durability {
    pub worker: WorkerConfig,
    pub usage_events: mazbri_billing::UsageEventsConfig,
    pub reconcile: mazbri_billing::ReconcileConfig,
}

pub async fn serve(
    addr: SocketAddr,
    db: Arc<PgPool>,
    db_read: Arc<PgPool>,
    service_name: String,
    tls_acceptor: Option<TlsAcceptor>,
    durability: Durability,
    max_connections: u32,
) -> Result<()> {
    let worker_config = durability.worker.clone();
    info!(
        "Starting tarpc billing server on {} (max connections: {})",
        addr, max_connections
    );
    let billing_service = Arc::new(BillingService::with_pgpools(
        db.clone(),
        db_read,
        BillingServiceConfig::default(),
    ));
    let node_id = hostname::get().map_or_else(
        |_| "unknown".to_string(),
        |h| h.to_string_lossy().to_string(),
    );
    let queue_config = QueueConfig {
        max_retries: worker_config.max_retries,
        retry_interval_secs: worker_config.retry_interval_secs,
        batch_size: worker_config.batch_size,
        max_concurrency: worker_config.max_concurrency,
        worker_loops: worker_config.worker_loops,
        idle_poll_ms: worker_config.idle_poll_ms,
        stale_processing_secs: worker_config.stale_processing_secs,
        worker_node_id: node_id,
    }
    .sanitized();
    let charge_queue = Arc::new(ChargeQueue::with_config(
        (*db).clone(),
        queue_config.clone(),
    ));
    let worker = if worker_config.enabled {
        let worker =
            ChargeWorker::with_config((*db).clone(), billing_service.clone(), queue_config.clone());
        worker.start().await;
        info!(
            batch_size = queue_config.batch_size,
            max_concurrency = queue_config.max_concurrency,
            worker_loops = queue_config.worker_loops,
            idle_poll_ms = queue_config.idle_poll_ms,
            "ChargeWorker started"
        );
        Some(worker)
    } else {
        info!("ChargeWorker disabled by configuration");
        None
    };
    let usage_outbox_worker = if worker_config.enabled {
        let worker = UsageOutboxWorker::with_config(
            (*db).clone(),
            billing_service.clone(),
            (*charge_queue).clone(),
            queue_config.clone(),
        );
        worker.start().await;
        info!(
            batch_size = queue_config.batch_size,
            max_concurrency = queue_config.max_concurrency,
            worker_loops = queue_config.worker_loops,
            idle_poll_ms = queue_config.idle_poll_ms,
            "UsageOutboxWorker started"
        );
        Some(worker)
    } else {
        info!("UsageOutboxWorker disabled by configuration");
        None
    };
    let (bg_shutdown_tx, bg_tasks) = spawn_durability_tasks(&durability, &db);
    let service = BillingTarpcService::new(service_name, billing_service, Some(charge_queue));
    mazbri_service::run_tarpc_server!(
        addr,
        service,
        tls_acceptor,
        max_connections,
        |svc: BillingTarpcService, ch| BaseChannel::with_defaults(ch).execute(svc.serve())
    )
    .await
    .map_err(|e| anyhow::anyhow!("tarpc server error: {e}"))?;
    let _ = bg_shutdown_tx.send(true);
    for t in bg_tasks {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), t).await;
    }
    if let Some(w) = usage_outbox_worker {
        w.stop().await;
    }
    if let Some(w) = worker {
        w.stop().await;
    }
    info!("计费服务关闭完成");
    Ok(())
}

/// Starts the L1 Redpanda consumer and the L3 reconcile job when enabled.
fn spawn_durability_tasks(
    durability: &Durability,
    db: &Arc<PgPool>,
) -> (
    tokio::sync::watch::Sender<bool>,
    Vec<tokio::task::JoinHandle<()>>,
) {
    let (bg_shutdown_tx, bg_shutdown_rx) = tokio::sync::watch::channel(false);
    let mut bg_tasks = Vec::new();
    if durability.usage_events.redpanda_enabled() {
        bg_tasks.push(
            mazbri_billing::UsageEventConsumer::new(
                durability.usage_events.clone(),
                (**db).clone(),
            )
            .spawn(bg_shutdown_rx.clone()),
        );
    } else {
        info!("Usage event consumer disabled (usage_events.sink != redpanda)");
    }
    if durability.reconcile.enabled {
        bg_tasks.push(
            mazbri_billing::ReconcileJob::new(durability.reconcile.clone(), (**db).clone())
                .spawn(bg_shutdown_rx),
        );
    } else {
        info!("Billing reconcile job disabled");
    }
    (bg_shutdown_tx, bg_tasks)
}
