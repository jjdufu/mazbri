use crate::error::BillingError;
use crate::metrics::BILLING_QUEUE_DEPTH;
use crate::types::{ChargeQueueItem, QueueConfig};
use chrono::{Duration as ChronoDuration, Utc};
use mazbri_core::num::SaturatingFrom;
use rust_decimal::Decimal;
use sqlx::PgPool;
use tracing::{debug, warn};
use uuid::Uuid;

/// Marks a usage record queued; never downgrades a record that is already charged.
const ENQUEUE_MARK_QUEUED_SQL: &str = "UPDATE usage_records SET charge_status = 'queued', charge_queue_id = $1 \
     WHERE id = $2 AND charge_status IS DISTINCT FROM 'charged'";
/// One queue row per usage record, ever (requires `uq_usage_charge_queue_usage_record_id`).
const ENQUEUE_INSERT_SQL: &str = "INSERT INTO usage_charge_queue \
     (tenant_id, account_id, usage_record_id, amount, payload, max_retries) \
     VALUES ($1, $2, $3, $4, $5, $6) \
     ON CONFLICT (usage_record_id) DO NOTHING RETURNING id";
#[derive(Clone)]
pub struct ChargeQueue {
    db: PgPool,
    config: QueueConfig,
}
impl ChargeQueue {
    #[must_use]
    pub fn new(db: PgPool) -> Self {
        Self {
            db,
            config: QueueConfig::default(),
        }
    }
    #[must_use]
    pub fn with_config(db: PgPool, config: QueueConfig) -> Self {
        Self { db, config }
    }
    #[must_use]
    pub fn config(&self) -> &QueueConfig {
        &self.config
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn enqueue(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        usage_record_id: i64,
        amount: Decimal,
        payload: Option<serde_json::Value>,
    ) -> Result<Uuid, BillingError> {
        let mut tx = self.db.begin().await?;
        let queue_id = self
            .enqueue_in_tx(
                &mut tx,
                tenant_id,
                account_id,
                usage_record_id,
                amount,
                payload,
            )
            .await?;
        tx.commit().await?;
        self.refresh_queue_depth_metrics().await;
        Ok(queue_id)
    }

    /// Idempotent enqueue inside the caller's transaction: at most one queue row per
    /// usage record (unique index on `usage_record_id`, `ON CONFLICT DO NOTHING`).
    /// The caller commits; pairing this with marking the outbox row processed in the
    /// same transaction prevents re-enqueue (double charge) after a crash.
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn enqueue_in_tx(
        &self,
        tx: &mut sqlx::PgConnection,
        tenant_id: Uuid,
        account_id: Uuid,
        usage_record_id: i64,
        amount: Decimal,
        payload: Option<serde_json::Value>,
    ) -> Result<Uuid, BillingError> {
        let inserted: Option<Uuid> = sqlx::query_scalar(ENQUEUE_INSERT_SQL)
            .bind(tenant_id)
            .bind(account_id)
            .bind(usage_record_id)
            .bind(amount)
            .bind(payload)
            .bind(self.config.max_retries)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(queue_id) = inserted else {
            let id: Uuid =
                sqlx::query_scalar("SELECT id FROM usage_charge_queue WHERE usage_record_id = $1")
                    .bind(usage_record_id)
                    .fetch_one(&mut *tx)
                    .await?;
            debug!(usage_record_id, queue_id = %id, "charge queue item already exists; skip");
            return Ok(id);
        };
        // Same transaction: the charge worker (FOR UPDATE SKIP LOCKED) cannot see the
        // queue row before usage_records.charge_queue_id is set (stale 'queued' race fix).
        sqlx::query(ENQUEUE_MARK_QUEUED_SQL)
            .bind(queue_id)
            .bind(usage_record_id)
            .execute(&mut *tx)
            .await?;
        debug!(
            "Enqueued charge: queue_id={}, tenant_id={}, usage_record_id={}, amount={}",
            queue_id, tenant_id, usage_record_id, amount
        );
        Ok(queue_id)
    }
    /// Reclaim rows left in `processing` after a crash (multi-replica safe).
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn reclaim_stale_processing(&self, stale_secs: u64) -> Result<u64, BillingError> {
        let result = sqlx::query(
            r"
            UPDATE usage_charge_queue
            SET status = 'pending',
                worker_node = NULL,
                locked_at = NULL,
                error_message = COALESCE(error_message, 'reclaimed stale processing')
            WHERE status = 'processing'
              AND locked_at IS NOT NULL
              AND locked_at < NOW() - ($1::double precision * INTERVAL '1 second')
            ",
        )
        .bind(mazbri_core::num::u64_to_f64(stale_secs))
        .execute(&self.db)
        .await?;
        let n = result.rows_affected();
        if n > 0 {
            warn!(
                reclaimed = n,
                stale_secs, "reclaimed stale usage_charge_queue processing rows"
            );
            self.refresh_queue_depth_metrics().await;
        }
        Ok(n)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn fetch_pending(&self, limit: i32) -> Result<Vec<ChargeQueueItem>, BillingError> {
        let mut tx = self.db.begin().await?;
        let rows = sqlx::query_as::<
            _,
            ChargeQueueItem,
        >(
                r"SELECT id, tenant_id, account_id, usage_record_id, amount, payload,
                      status::VARCHAR as status, retry_count, max_retries, error_message, scheduled_at,
                      transaction_id, created_at
               FROM usage_charge_queue
               WHERE status IN ('pending', 'failed')
                 AND scheduled_at <= NOW()
               ORDER BY created_at ASC
               LIMIT $1
               FOR UPDATE SKIP LOCKED",
            )
            .bind(i64::from(limit))
            .fetch_all(&mut *tx)
            .await?;
        if rows.is_empty() {
            tx.commit().await?;
            return Ok(Vec::new());
        }
        let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
        let now = Utc::now();
        sqlx::query(
            r"UPDATE usage_charge_queue
               SET status = 'processing', worker_node = $1, locked_at = $2
               WHERE id = ANY($3)",
        )
        .bind(&self.config.worker_node_id)
        .bind(now)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        debug!("Fetched {} pending charge items for processing", rows.len());
        self.refresh_queue_depth_metrics().await;
        Ok(rows)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn mark_completed(
        &self,
        queue_id: Uuid,
        transaction_id: Uuid,
    ) -> Result<(), BillingError> {
        let now = Utc::now();
        sqlx::query(
            r"UPDATE usage_charge_queue
               SET status = 'completed', transaction_id = $2, processed_at = $3
               WHERE id = $1",
        )
        .bind(queue_id)
        .bind(transaction_id)
        .bind(now)
        .execute(&self.db)
        .await?;
        sqlx::query(
            "UPDATE usage_records SET charge_status = 'charged', charged_at = $1 \
             WHERE charge_queue_id = $2",
        )
        .bind(now)
        .bind(queue_id)
        .execute(&self.db)
        .await?;
        debug!("Marked charge queue item as completed: {}", queue_id);
        self.refresh_queue_depth_metrics().await;
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn mark_failed(
        &self,
        queue_id: Uuid,
        error_message: &str,
    ) -> Result<bool, BillingError> {
        let (retry_count, max_retries): (i32, i32) =
            sqlx::query_as("SELECT retry_count, max_retries FROM usage_charge_queue WHERE id = $1")
                .bind(queue_id)
                .fetch_one(&self.db)
                .await?;
        if retry_count < max_retries {
            const MAX_RETRY_DELAY_SECS: i64 = 3600;
            let backoff_factor = 2i64.pow(u32::saturating_from(retry_count.min(30)));
            let delay_secs =
                (self.config.retry_interval_secs * backoff_factor).min(MAX_RETRY_DELAY_SECS);
            let scheduled_at = Utc::now() + ChronoDuration::seconds(delay_secs);
            sqlx::query(
                r"UPDATE usage_charge_queue
                   SET status = 'failed', retry_count = retry_count + 1,
                       error_message = $1, scheduled_at = $2
                   WHERE id = $3",
            )
            .bind(error_message)
            .bind(scheduled_at)
            .bind(queue_id)
            .execute(&self.db)
            .await?;
            sqlx::query(
                    "UPDATE usage_records SET charge_status = 'charge_failed' WHERE charge_queue_id = $1",
                )
                .bind(queue_id)
                .execute(&self.db)
                .await?;
            warn!(
                "Charge queue item {} failed (attempt {}/{}), will retry at {}",
                queue_id,
                retry_count + 1,
                max_retries,
                scheduled_at
            );
            self.refresh_queue_depth_metrics().await;
            Ok(true)
        } else {
            sqlx::query(
                    r"UPDATE usage_charge_queue SET status = 'dead_letter', error_message = $1 WHERE id = $2",
                )
                .bind(error_message)
                .bind(queue_id)
                .execute(&self.db)
                .await?;
            let item: ChargeQueueItem = sqlx::query_as(
                "SELECT id, tenant_id, account_id, usage_record_id, amount, payload, \
                 status::VARCHAR as status, retry_count, max_retries, error_message, scheduled_at, \
                 transaction_id, created_at FROM usage_charge_queue WHERE id = $1",
            )
            .bind(queue_id)
            .fetch_one(&self.db)
            .await?;
            sqlx::query(
                r"INSERT INTO charge_failures
                   (queue_id, tenant_id, account_id, amount, error_message, attempt_number)
                   VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(queue_id)
            .bind(item.tenant_id)
            .bind(item.account_id)
            .bind(item.amount)
            .bind(error_message)
            .bind(item.retry_count)
            .execute(&self.db)
            .await?;
            warn!(
                "Charge queue item {} moved to dead letter after {} attempts",
                queue_id, max_retries
            );
            self.refresh_queue_depth_metrics().await;
            Ok(false)
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_stats(&self) -> Result<QueueStats, BillingError> {
        let pending: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM usage_charge_queue WHERE status = 'pending'")
                .fetch_one(&self.db)
                .await?;
        let processing: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM usage_charge_queue WHERE status = 'processing'",
        )
        .fetch_one(&self.db)
        .await?;
        let failed: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM usage_charge_queue WHERE status = 'failed'")
                .fetch_one(&self.db)
                .await?;
        let dead_letter: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM usage_charge_queue WHERE status = 'dead_letter'",
        )
        .fetch_one(&self.db)
        .await?;
        let stats = QueueStats {
            pending,
            processing,
            failed,
            dead_letter,
        };
        stats.record_metrics();
        Ok(stats)
    }
    async fn refresh_queue_depth_metrics(&self) {
        if let Ok(stats) = self.get_stats().await {
            stats.record_metrics();
        }
    }
}
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct QueueStats {
    pub pending: i64,
    pub processing: i64,
    pub failed: i64,
    pub dead_letter: i64,
}
impl QueueStats {
    pub fn record_metrics(&self) {
        BILLING_QUEUE_DEPTH
            .with_label_values(&["pending"])
            .set(self.pending);
        BILLING_QUEUE_DEPTH
            .with_label_values(&["processing"])
            .set(self.processing);
        BILLING_QUEUE_DEPTH
            .with_label_values(&["failed"])
            .set(self.failed);
        BILLING_QUEUE_DEPTH
            .with_label_values(&["dead_letter"])
            .set(self.dead_letter);
    }
}

#[cfg(test)]
mod enqueue_sql_tests {
    use super::{ENQUEUE_INSERT_SQL, ENQUEUE_MARK_QUEUED_SQL};

    #[test]
    fn insert_is_idempotent_per_usage_record() {
        assert!(ENQUEUE_INSERT_SQL.contains("ON CONFLICT (usage_record_id) DO NOTHING"));
        assert!(ENQUEUE_INSERT_SQL.contains("RETURNING id"));
    }

    #[test]
    fn mark_queued_never_downgrades_charged() {
        assert!(ENQUEUE_MARK_QUEUED_SQL.contains("charge_status IS DISTINCT FROM 'charged'"));
        assert!(ENQUEUE_MARK_QUEUED_SQL.contains("WHERE id = $2"));
    }
}
