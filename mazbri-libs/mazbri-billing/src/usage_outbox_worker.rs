use crate::error::BillingError;
use crate::queue::ChargeQueue;
use crate::service::BillingService;
use crate::types::{QueueConfig, RecordUsageOutcome};
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
use tracing::{Instrument, debug, error, info, warn};
use uuid::Uuid;

const CLAIM_PENDING_OUTBOX_SQL: &str = r"
WITH claimed AS (
    SELECT id
    FROM usage_outbox
    WHERE status IN ('pending', 'failed')
      AND next_attempt_at <= NOW()
    ORDER BY created_at ASC
    LIMIT $1
    FOR UPDATE SKIP LOCKED
)
UPDATE usage_outbox o
SET status = 'processing',
    lease_seq = o.lease_seq + 1,
    updated_at = NOW()
FROM claimed
WHERE o.id = claimed.id
RETURNING
    o.id, o.event_id, o.request_id, o.tenant_id, o.account_id, o.product_id,
    o.api_endpoint, o.http_method, o.response_status, o.request_size,
    o.response_size, o.billing_service_duration_ms, o.unit_price,
    o.billable_units, o.billable_amount, o.billing_code, o.trace_parent,
    o.lease_seq
";

/// Marks one outbox row processed with monotonic `lease_seq` fencing; runs in the same
/// transaction as `usage_records` upsert and `usage_charge_queue` enqueue.
const MARK_OUTBOX_PROCESSED_SQL: &str = "UPDATE usage_outbox SET status = 'processed', \
     processed_at = NOW(), error_message = NULL \
     WHERE id = $1 AND status = 'processing' AND lease_seq = $2";

const MARK_OUTBOX_FAILED_LOCK_SQL: &str = "SELECT retry_count FROM usage_outbox \
     WHERE id = $1 AND status = 'processing' AND lease_seq = $2 FOR UPDATE";

const MARK_OUTBOX_DEAD_LETTER_SQL: &str = r"
UPDATE usage_outbox
SET status = 'dead_letter',
    retry_count = retry_count + 1,
    error_message = $2
WHERE id = $1 AND status = 'processing' AND lease_seq = $3
";

const MARK_OUTBOX_RETRY_SQL: &str = r"
UPDATE usage_outbox
SET status = 'failed',
    retry_count = retry_count + 1,
    next_attempt_at = $2,
    error_message = $3
WHERE id = $1 AND status = 'processing' AND lease_seq = $4
";

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
    trace_parent: Option<String>,
    lease_seq: i64,
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
    let batch_span = tracing::info_span!(parent: None, "billing.outbox_batch", batch = batch_len);
    mazbri_core::trace_spans::add_links(
        &batch_span,
        items.iter().filter_map(|item| item.trace_parent.as_deref()),
    );
    let linked_trace_ids = mazbri_core::trace_spans::linked_trace_ids(
        items.iter().filter_map(|item| item.trace_parent.as_deref()),
    );
    let results: Vec<_> = stream::iter(items)
        .map(|item| {
            let db = db.clone();
            let billing_service = billing_service.clone();
            let charge_queue = charge_queue.clone();
            async move {
                let item_id = item.id;
                let lease_seq = item.lease_seq;
                let result = process_one(&db, &billing_service, &charge_queue, item).await;
                (item_id, lease_seq, result)
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .instrument(batch_span)
        .await;

    let mut ok_ids = Vec::new();
    let mut fail_count = 0usize;
    for (item_id, lease_seq, result) in results {
        match result {
            Ok(()) => ok_ids.push(item_id),
            Err(error) => {
                fail_count += 1;
                warn!(item_id, lease_seq, error = %error, "UsageOutbox item failed");
                mark_failed(db, item_id, lease_seq, &error.to_string(), config).await?;
            }
        }
    }
    info!(
        batch = batch_len,
        success = ok_ids.len(),
        failed = fail_count,
        elapsed_ms = u64::saturating_from(started.elapsed().as_millis()),
        linked_trace_ids,
        "usage_outbox batch done"
    );
    Ok(batch_len)
}

async fn fetch_pending(db: &PgPool, limit: i32) -> Result<Vec<UsageOutboxItem>, BillingError> {
    let rows = sqlx::query_as::<_, UsageOutboxItem>(CLAIM_PENDING_OUTBOX_SQL)
        .bind(i64::from(limit))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

async fn process_one(
    db: &PgPool,
    billing_service: &Arc<BillingService>,
    charge_queue: &ChargeQueue,
    item: UsageOutboxItem,
) -> Result<(), BillingError> {
    if item.billable_amount < Decimal::ZERO {
        return Err(BillingError::InvalidAmount(format!(
            "Outbox billable_amount must be non-negative, got {}",
            item.billable_amount
        )));
    }
    let account_id = match item.account_id {
        Some(account_id) => account_id,
        None => billing_service
            .try_get_account_id(item.tenant_id)
            .await?
            .ok_or(BillingError::AccountNotFound(item.tenant_id))?,
    };
    let mut tx = db.begin().await?;
    // Step 1: Monotonic lease fence on usage_outbox inside the same transaction.
    let fenced = sqlx::query(MARK_OUTBOX_PROCESSED_SQL)
        .bind(item.id)
        .bind(item.lease_seq)
        .execute(&mut *tx)
        .await?;
    if fenced.rows_affected() == 0 {
        tx.commit().await?;
        debug!(
            item_id = item.id,
            event_id = %item.event_id,
            request_id = %item.request_id,
            lease_seq = item.lease_seq,
            "usage_outbox row fenced out (stale lease_seq); skipping"
        );
        return Ok(());
    }
    // Step 2: Idempotently insert or fetch usage_records row by event_id in the same tx.
    let units = item.billable_units.max(0);
    let inserted_id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO usage_records (
            tenant_id, account_id, api_endpoint, http_method, response_status,
            request_size, response_size, billing_service_duration_ms,
            unit_price, billable_amount, billable_units,
            product_id, billing_code, event_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT (event_id) WHERE event_id IS NOT NULL DO NOTHING
        RETURNING id",
    )
    .bind(item.tenant_id)
    .bind(account_id)
    .bind(&item.api_endpoint)
    .bind(&item.http_method)
    .bind(item.response_status)
    .bind(item.request_size)
    .bind(item.response_size)
    .bind(item.billing_service_duration_ms)
    .bind(item.unit_price)
    .bind(item.billable_amount)
    .bind(units)
    .bind(&item.product_id)
    .bind(item.billing_code)
    .bind(item.event_id)
    .fetch_optional(&mut *tx)
    .await?;

    let usage_record_id = match inserted_id {
        Some(id) => id,
        None => {
            sqlx::query_scalar::<_, i64>("SELECT id FROM usage_records WHERE event_id = $1")
                .bind(item.event_id)
                .fetch_one(&mut *tx)
                .await?
        }
    };

    // Step 3: Enqueue charge in the same transaction when billable_amount > 0.
    if item.billable_amount > Decimal::ZERO {
        let _ = RecordUsageOutcome::Queued {
            usage_record_id,
            queue_id: charge_queue
                .enqueue_in_tx(
                    &mut tx,
                    crate::queue::ChargeEnqueue {
                        tenant_id: item.tenant_id,
                        account_id,
                        usage_record_id,
                        amount: item.billable_amount,
                        payload: None,
                        trace_parent: item.trace_parent.as_deref(),
                    },
                )
                .await?,
        };
    }
    tx.commit().await?;
    Ok(())
}

async fn mark_failed(
    db: &PgPool,
    id: i64,
    lease_seq: i64,
    error: &str,
    config: &QueueConfig,
) -> Result<(), BillingError> {
    let mut tx = db.begin().await?;
    let retry_count: Option<i32> = sqlx::query_scalar(MARK_OUTBOX_FAILED_LOCK_SQL)
        .bind(id)
        .bind(lease_seq)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(retry_count) = retry_count else {
        tx.commit().await?;
        debug!(
            id,
            lease_seq, "mark_failed fenced out: usage_outbox row no longer owned by lease"
        );
        return Ok(());
    };
    let next_retry = retry_count + 1;
    if next_retry >= config.max_retries {
        sqlx::query(MARK_OUTBOX_DEAD_LETTER_SQL)
            .bind(id)
            .bind(error)
            .bind(lease_seq)
            .execute(&mut *tx)
            .await?;
    } else {
        let backoff_factor = 2i64.pow(u32::saturating_from(retry_count.clamp(0, 30)));
        let delay_secs = (config.retry_interval_secs * backoff_factor).min(3600);
        let next_attempt_at = Utc::now() + ChronoDuration::seconds(delay_secs);
        sqlx::query(MARK_OUTBOX_RETRY_SQL)
            .bind(id)
            .bind(next_attempt_at)
            .bind(error)
            .bind(lease_seq)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
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
    use super::{
        CLAIM_PENDING_OUTBOX_SQL, MARK_OUTBOX_DEAD_LETTER_SQL, MARK_OUTBOX_FAILED_LOCK_SQL,
        MARK_OUTBOX_PROCESSED_SQL, MARK_OUTBOX_RETRY_SQL,
    };

    #[test]
    fn outbox_sql_enforces_monotonic_lease_seq_fence() {
        assert!(CLAIM_PENDING_OUTBOX_SQL.contains("lease_seq = o.lease_seq + 1"));
        assert!(CLAIM_PENDING_OUTBOX_SQL.contains("o.lease_seq"));
        assert!(MARK_OUTBOX_PROCESSED_SQL.contains("status = 'processed'"));
        assert!(
            MARK_OUTBOX_PROCESSED_SQL
                .contains("WHERE id = $1 AND status = 'processing' AND lease_seq = $2")
        );
        assert!(
            MARK_OUTBOX_FAILED_LOCK_SQL
                .contains("WHERE id = $1 AND status = 'processing' AND lease_seq = $2")
        );
        assert!(
            MARK_OUTBOX_DEAD_LETTER_SQL
                .contains("WHERE id = $1 AND status = 'processing' AND lease_seq = $3")
        );
        assert!(
            MARK_OUTBOX_RETRY_SQL
                .contains("WHERE id = $1 AND status = 'processing' AND lease_seq = $4")
        );
    }
}
