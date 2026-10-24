use crate::calculator::CalculationResult;
use crate::error::BillingError;
use crate::queue::ChargeQueue;
use crate::service::{BillingService, UsageRecord};
use crate::types::QueueConfig;
use chrono::{Duration as ChronoDuration, Utc};
use futures::stream::{self, StreamExt};
use mazbri_core::num::SaturatingFrom;
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Marks one outbox row processed; runs in the same transaction as the enqueue.
const MARK_OUTBOX_PROCESSED_SQL: &str = "UPDATE usage_outbox SET status = 'processed', \
     processed_at = NOW(), error_message = NULL WHERE id = $1";

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

#[derive(Debug, Clone, sqlx::FromRow)]
struct UsageOutboxItem {
    id: i64,
    event_id: Uuid,
    request_id: Uuid,
    tenant_id: Uuid,
    account_id: Option<Uuid>,
    product_id: Option<String>,
    api_endpoint: String,
    http_method: String,
    response_status: i32,
    request_size: i64,
    response_size: i64,
    billing_service_duration_ms: i32,
    unit_price: Decimal,
    billable_units: i64,
    billable_amount: Decimal,
    billing_code: Option<i16>,
}

pub struct UsageOutboxWorker {
    db: PgPool,
    charge_queue: ChargeQueue,
    billing_service: Arc<BillingService>,
    config: QueueConfig,
    running: Arc<AtomicBool>,
    task_state: Arc<Mutex<TaskState>>,
}

impl UsageOutboxWorker {
    #[must_use]
    pub fn with_config(
        db: PgPool,
        billing_service: Arc<BillingService>,
        charge_queue: ChargeQueue,
        config: QueueConfig,
    ) -> Self {
        Self {
            db,
            charge_queue,
            billing_service,
            config: config.sanitized(),
            running: Arc::new(AtomicBool::new(false)),
            task_state: Arc::new(Mutex::new(TaskState::new())),
        }
    }

    pub async fn start(&self) {
        if self.running.load(Ordering::Acquire) {
            info!("UsageOutboxWorker already running");
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
            "UsageOutboxWorker starting (continuous drain)"
        );

        let mut handles = Vec::with_capacity(loops);
        for loop_id in 0..loops {
            let db = self.db.clone();
            let charge_queue = self.charge_queue.clone();
            let billing_service = self.billing_service.clone();
            let config = self.config.clone();
            let running = self.running.clone();
            let mut shutdown_rx = shutdown_tx.subscribe();
            handles.push(tokio::spawn(async move {
                run_outbox_loop(
                    loop_id,
                    db,
                    billing_service,
                    charge_queue,
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
            info!("UsageOutboxWorker shutdown signal sent");
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

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn shutdown(self, timeout: Duration) -> Result<(), tokio::time::error::Elapsed> {
        let stop_fut = self.stop();
        tokio::time::timeout(timeout, stop_fut).await
    }
}

async fn run_outbox_loop(
    loop_id: usize,
    db: PgPool,
    billing_service: Arc<BillingService>,
    charge_queue: ChargeQueue,
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

        // Opportunistic stale reclaim (once per loop roughly every stale window).
        if reclaim_ticker.elapsed() >= Duration::from_secs(config.stale_processing_secs) {
            if let Err(error) = reclaim_stale_outbox(&db, config.stale_processing_secs).await {
                warn!(loop_id, error = %error, "outbox stale reclaim failed");
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
            result = process_batch(
                &db,
                &billing_service,
                &charge_queue,
                &config,
            ) => result,
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
                debug!(loop_id, processed = n, "outbox batch drained");
            }
            Err(error) => {
                error!(loop_id, error = %error, "UsageOutboxWorker batch error");
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
    info!(loop_id, "UsageOutboxWorker loop stopped");
}

async fn reclaim_stale_outbox(db: &PgPool, stale_secs: u64) -> Result<u64, BillingError> {
    let result = sqlx::query(
        r"
        UPDATE usage_outbox
        SET status = 'pending',
            error_message = COALESCE(error_message, 'reclaimed stale processing')
        WHERE status = 'processing'
          AND updated_at < NOW() - ($1::double precision * INTERVAL '1 second')
        ",
    )
    .bind(mazbri_core::num::u64_to_f64(stale_secs))
    .execute(db)
    .await?;
    let n = result.rows_affected();
    if n > 0 {
        warn!(
            reclaimed = n,
            stale_secs, "reclaimed stale usage_outbox processing rows"
        );
    }
    Ok(n)
}

async fn process_batch(
    db: &PgPool,
    billing_service: &Arc<BillingService>,
    charge_queue: &ChargeQueue,
    config: &QueueConfig,
) -> Result<usize, BillingError> {
    let items = fetch_pending(db, config.batch_size).await?;
    if items.is_empty() {
        return Ok(0);
    }
    let batch_len = items.len();
    let started = Instant::now();
    let concurrency = config.max_concurrency.max(1);
    let results: Vec<_> = stream::iter(items)
        .map(|item| {
            let db = db.clone();
            let billing_service = billing_service.clone();
            let charge_queue = charge_queue.clone();
            async move {
                let item_id = item.id;
                let result = process_one(&db, &billing_service, &charge_queue, item).await;
                (item_id, result)
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let mut ok_ids = Vec::new();
    let mut fail_count = 0usize;
    for (item_id, result) in results {
        match result {
            Ok(()) => ok_ids.push(item_id),
            Err(error) => {
                fail_count += 1;
                warn!(item_id, error = %error, "UsageOutbox item failed");
                mark_failed(db, item_id, &error.to_string(), config).await?;
            }
        }
    }
    info!(
        batch = batch_len,
        success = ok_ids.len(),
        failed = fail_count,
        elapsed_ms = u64::saturating_from(started.elapsed().as_millis()),
        "usage_outbox batch done"
    );
    Ok(batch_len)
}

async fn fetch_pending(db: &PgPool, limit: i32) -> Result<Vec<UsageOutboxItem>, BillingError> {
    let mut tx = db.begin().await?;
    let rows = sqlx::query_as::<_, UsageOutboxItem>(
        r"
        SELECT
            id, event_id, request_id, tenant_id, account_id, product_id,
            api_endpoint, http_method, response_status, request_size,
            response_size, billing_service_duration_ms, unit_price,
            billable_units, billable_amount, billing_code
        FROM usage_outbox
        WHERE status IN ('pending', 'failed')
          AND next_attempt_at <= NOW()
        ORDER BY created_at ASC
        LIMIT $1
        FOR UPDATE SKIP LOCKED
        ",
    )
    .bind(i64::from(limit))
    .fetch_all(&mut *tx)
    .await?;
    if rows.is_empty() {
        tx.commit().await?;
        return Ok(Vec::new());
    }
    let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
    sqlx::query(
        r"
        UPDATE usage_outbox
        SET status = 'processing'
        WHERE id = ANY($1)
        ",
    )
    .bind(&ids)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows)
}

async fn process_one(
    db: &PgPool,
    billing_service: &Arc<BillingService>,
    charge_queue: &ChargeQueue,
    item: UsageOutboxItem,
) -> Result<(), BillingError> {
    let account_id = match item.account_id {
        Some(account_id) => account_id,
        None => billing_service
            .get_account_id(item.tenant_id)
            .await
            .ok_or(BillingError::AccountNotFound(item.tenant_id))?,
    };
    let usage_record_id = if let Some(existing_id) =
        sqlx::query_scalar::<_, i64>("SELECT id FROM usage_records WHERE event_id = $1")
            .bind(item.event_id)
            .fetch_optional(db)
            .await?
    {
        existing_id
    } else {
        let usage_record = UsageRecord {
            event_id: Some(item.event_id),
            tenant_id: item.tenant_id,
            account_id,
            api_endpoint: item.api_endpoint.clone(),
            http_method: item.http_method.clone(),
            response_status: u16::saturating_from(item.response_status),
            request_size: item.request_size,
            response_size: item.response_size,
            billing_service_duration_ms: item.billing_service_duration_ms,
            billing_result: CalculationResult {
                amount: item.billable_amount,
                units: u64::saturating_from(item.billable_units.max(0)),
                unit_price: item.unit_price,
                description: format!(
                    "usage_outbox request_id={} endpoint={}",
                    item.request_id, item.api_endpoint
                ),
            },
            product_id: item.product_id.clone(),
            billing_code: item.billing_code,
        };
        billing_service.record_usage(usage_record).await?
    };
    // Enqueue and mark the outbox row processed atomically: a crash can no longer leave
    // the charge enqueued while the outbox row is reclaimed and re-enqueued.
    let mut tx = db.begin().await?;
    if item.billable_amount > Decimal::ZERO {
        charge_queue
            .enqueue_in_tx(
                &mut tx,
                item.tenant_id,
                account_id,
                usage_record_id,
                item.billable_amount,
                None,
            )
            .await?;
    }
    sqlx::query(MARK_OUTBOX_PROCESSED_SQL)
        .bind(item.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn mark_failed(
    db: &PgPool,
    id: i64,
    error: &str,
    config: &QueueConfig,
) -> Result<(), BillingError> {
    let retry_count: i32 = sqlx::query_scalar("SELECT retry_count FROM usage_outbox WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await?;
    let next_retry = retry_count + 1;
    if next_retry >= config.max_retries {
        sqlx::query(
            r"
            UPDATE usage_outbox
            SET status = 'dead_letter',
                retry_count = retry_count + 1,
                error_message = $2
            WHERE id = $1
            ",
        )
        .bind(id)
        .bind(error)
        .execute(db)
        .await?;
    } else {
        let backoff_factor = 2i64.pow(u32::saturating_from(retry_count.clamp(0, 30)));
        let delay_secs = (config.retry_interval_secs * backoff_factor).min(3600);
        let next_attempt_at = Utc::now() + ChronoDuration::seconds(delay_secs);
        sqlx::query(
            r"
            UPDATE usage_outbox
            SET status = 'failed',
                retry_count = retry_count + 1,
                next_attempt_at = $2,
                error_message = $3
            WHERE id = $1
            ",
        )
        .bind(id)
        .bind(next_attempt_at)
        .bind(error)
        .execute(db)
        .await?;
    }
    Ok(())
}

impl Clone for UsageOutboxWorker {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            charge_queue: self.charge_queue.clone(),
            billing_service: self.billing_service.clone(),
            config: self.config.clone(),
            running: self.running.clone(),
            task_state: self.task_state.clone(),
        }
    }
}

#[cfg(test)]
mod outbox_sql_tests {
    use super::MARK_OUTBOX_PROCESSED_SQL;

    #[test]
    fn mark_processed_targets_single_row() {
        assert!(MARK_OUTBOX_PROCESSED_SQL.contains("status = 'processed'"));
        assert!(MARK_OUTBOX_PROCESSED_SQL.contains("WHERE id = $1"));
    }
}
