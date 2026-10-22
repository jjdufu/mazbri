use crate::error::BillingError;
use crate::queue::ChargeQueue;
use crate::service::BillingService;
use crate::types::{ChargeQueueItem, QueueConfig};
use futures::stream::{self, StreamExt};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

struct TaskState {
    handles: Vec<JoinHandle<()>>,
    shutdown_tx: Option<watch::Sender<bool>>,
}

impl TaskState {
    fn new() -> Self {
        Self {
            handles: Vec::new(),
            shutdown_tx: None,
        }
    }
}

pub struct ChargeWorker {
    queue: ChargeQueue,
    billing_service: Arc<BillingService>,
    config: QueueConfig,
    running: Arc<AtomicBool>,
    task_state: Arc<Mutex<TaskState>>,
}

impl ChargeWorker {
    #[must_use]
    pub fn new(db: PgPool, billing_service: Arc<BillingService>) -> Self {
        Self::with_config(db, billing_service, QueueConfig::default())
    }

    #[must_use]
    pub fn with_config(
        db: PgPool,
        billing_service: Arc<BillingService>,
        config: QueueConfig,
    ) -> Self {
        let config = config.sanitized();
        Self {
            queue: ChargeQueue::with_config(db, config.clone()),
            billing_service,
            config,
            running: Arc::new(AtomicBool::new(false)),
            task_state: Arc::new(Mutex::new(TaskState::new())),
        }
    }

    pub async fn start(&self) {
        if self.running.load(Ordering::Acquire) {
            info!("ChargeWorker already running");
            return;
        }
        self.running.store(true, Ordering::Release);
        let (shutdown_tx, _) = watch::channel(false);
        let loops = self.config.worker_loops;
        info!(
            batch_size = self.config.batch_size,
            max_concurrency = self.config.max_concurrency,
            worker_loops = loops,
            idle_poll_ms = self.config.idle_poll_ms,
            stale_processing_secs = self.config.stale_processing_secs,
            "ChargeWorker starting (continuous drain, tenant-batched charges)"
        );

        let mut handles = Vec::with_capacity(loops);
        for loop_id in 0..loops {
            let queue = self.queue.clone();
            let billing_service = self.billing_service.clone();
            let config = self.config.clone();
            let running = self.running.clone();
            let mut shutdown_rx = shutdown_tx.subscribe();
            handles.push(tokio::spawn(async move {
                run_charge_loop(
                    loop_id,
                    queue,
                    billing_service,
                    config,
                    running,
                    &mut shutdown_rx,
                )
                .await;
            }));
        }

        let mut task_state = self.task_state.lock().await;
        task_state.handles = handles;
        task_state.shutdown_tx = Some(shutdown_tx);
    }

    pub async fn stop(&self) {
        let shutdown_tx = self.task_state.lock().await.shutdown_tx.take();
        if let Some(tx) = shutdown_tx {
            let _ = tx.send(true);
            info!("ChargeWorker shutdown signal sent");
        }
        self.running.store(false, Ordering::Release);
        let handles = {
            let mut task_state = self.task_state.lock().await;
            std::mem::take(&mut task_state.handles)
        };
        for handle in handles {
            let _ = handle.await;
        }
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn shutdown(self, timeout: Duration) -> Result<(), tokio::time::error::Elapsed> {
        let stop_fut = self.stop();
        tokio::time::timeout(timeout, stop_fut).await
    }
}

async fn run_charge_loop(
    loop_id: usize,
    queue: ChargeQueue,
    billing_service: Arc<BillingService>,
    config: QueueConfig,
    running: Arc<AtomicBool>,
    shutdown_rx: &mut watch::Receiver<bool>,
) {
    let idle = Duration::from_millis(config.idle_poll_ms);
    let mut reclaim_ticker = Instant::now()
        .checked_sub(Duration::from_secs(config.stale_processing_secs))
        .unwrap_or_else(Instant::now);

    loop {
        if !running.load(Ordering::Relaxed) || *shutdown_rx.borrow() {
            break;
        }

        if reclaim_ticker.elapsed() >= Duration::from_secs(config.stale_processing_secs) {
            if let Err(error) = queue
                .reclaim_stale_processing(config.stale_processing_secs)
                .await
            {
                warn!(loop_id, error = %error, "charge queue stale reclaim failed");
            }
            reclaim_ticker = Instant::now();
        }

        let batch_result = tokio::select! {
            biased;
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
                continue;
            }
            result = process_batch(&queue, &billing_service, &config) => result,
        };

        match batch_result {
            Ok(0) => {
                tokio::select! {
                    biased;
                    changed = shutdown_rx.changed() => {
                        if changed.is_err() || *shutdown_rx.borrow() {
                            break;
                        }
                    }
                    () = sleep(idle) => {}
                }
            }
            Ok(n) => {
                debug!(loop_id, processed = n, "charge batch drained");
            }
            Err(error) => {
                error!(loop_id, error = %error, "ChargeWorker batch error");
                tokio::select! {
                    biased;
                    changed = shutdown_rx.changed() => {
                        if changed.is_err() || *shutdown_rx.borrow() {
                            break;
                        }
                    }
                    () = sleep(idle) => {}
                }
            }
        }
    }
    info!(loop_id, "ChargeWorker loop stopped");
}

async fn process_batch(
    queue: &ChargeQueue,
    billing_service: &Arc<BillingService>,
    config: &QueueConfig,
) -> Result<usize, BillingError> {
    let items = queue.fetch_pending(config.batch_size).await?;
    if items.is_empty() {
        return Ok(0);
    }
    let batch_len = items.len();
    let started = Instant::now();

    // Group by tenant: one balance UPDATE per tenant group (kills single-tenant row-lock stampede).
    let mut by_tenant: HashMap<Uuid, Vec<ChargeQueueItem>> = HashMap::new();
    for item in items {
        by_tenant.entry(item.tenant_id).or_default().push(item);
    }
    let tenant_groups: Vec<(Uuid, Vec<ChargeQueueItem>)> = by_tenant.into_iter().collect();
    let tenant_count = tenant_groups.len();
    let concurrency = config.max_concurrency.max(1);

    let results: Vec<_> = stream::iter(tenant_groups)
        .map(|(tenant_id, group)| {
            let billing_service = billing_service.clone();
            let queue = queue.clone();
            async move { process_tenant_group(billing_service, queue, tenant_id, group).await }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let mut success = 0usize;
    let mut failed = 0usize;
    for (s, f) in results {
        success += s;
        failed += f;
    }
    info!(
        batch = batch_len,
        tenants = tenant_count,
        success,
        failed,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "charge queue batch done (tenant-grouped)"
    );
    Ok(batch_len)
}

async fn process_tenant_group(
    billing_service: Arc<BillingService>,
    queue: ChargeQueue,
    tenant_id: Uuid,
    group: Vec<ChargeQueueItem>,
) -> (usize, usize) {
    if group.is_empty() {
        return (0, 0);
    }
    let account_id = group[0].account_id;
    let items: Vec<(Uuid, i64, Decimal)> = group
        .iter()
        .map(|i| (i.id, i.usage_record_id, i.amount))
        .collect();
    let item_count = items.len();

    match billing_service
        .deduct_and_record_batch(tenant_id, account_id, &items)
        .await
    {
        Ok(result) => {
            debug!(
                %tenant_id,
                items = item_count,
                balance_after = %result.balance_after,
                "tenant charge batch completed"
            );
            (item_count, 0)
        }
        Err(error) => {
            // Insufficient total or transient batch failure: fall back to per-item so partial
            // progress is still possible and individual failures are marked correctly.
            warn!(
                %tenant_id,
                items = item_count,
                error = %error,
                "tenant charge batch failed; falling back to per-item charges"
            );
            let mut success = 0usize;
            let mut failed = 0usize;
            for item in group {
                let item_id = item.id;
                match billing_service
                    .deduct_and_record(
                        item.tenant_id,
                        item.amount,
                        item.account_id,
                        item.usage_record_id,
                        item.id,
                    )
                    .await
                {
                    Ok(_) => success += 1,
                    Err(e) => {
                        failed += 1;
                        let error_msg = e.to_string();
                        warn!(item_id = %item_id, error = %error_msg, "charge failed");
                        if let Err(mark_err) = queue.mark_failed(item_id, &error_msg).await {
                            error!(
                                item_id = %item_id,
                                error = %mark_err,
                                "failed to mark charge queue item failed"
                            );
                        }
                    }
                }
            }
            (success, failed)
        }
    }
}

impl Clone for ChargeWorker {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            billing_service: self.billing_service.clone(),
            config: self.config.clone(),
            running: self.running.clone(),
            task_state: self.task_state.clone(),
        }
    }
}
