use crate::error::BillingError;
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
     (tenant_id, account_id, usage_record_id, amount, payload, max_retries, trace_parent) \
     VALUES ($1, $2, $3, $4, $5, $6, $7) \
     ON CONFLICT (usage_record_id) DO NOTHING RETURNING id";
const CLAIM_PENDING_QUEUE_SQL: &str = r"
WITH claimed AS (
    SELECT id
    FROM usage_charge_queue
    WHERE status IN ('pending', 'failed')
      AND scheduled_at <= NOW()
    ORDER BY created_at ASC
    LIMIT $1
    FOR UPDATE SKIP LOCKED
)
UPDATE usage_charge_queue q
SET status = 'processing',
    worker_node = $2,
    locked_at = NOW(),
    lease_seq = q.lease_seq + 1
FROM claimed
WHERE q.id = claimed.id
RETURNING q.id, q.tenant_id, q.account_id, q.usage_record_id, q.amount, q.payload,
          q.status::VARCHAR as status, q.lease_seq, q.retry_count, q.max_retries,
          q.error_message, q.scheduled_at, q.transaction_id, q.created_at, q.trace_parent
";
const MARK_COMPLETED_FENCED_SQL: &str = r"UPDATE usage_charge_queue
   SET status = 'completed', transaction_id = $2, processed_at = $3
   WHERE id = $1 AND status = 'processing' AND lease_seq = $4";
const MARK_FAILED_LOCK_FENCED_SQL: &str = r"SELECT tenant_id, account_id, amount, retry_count, max_retries
   FROM usage_charge_queue
   WHERE id = $1 AND status = 'processing' AND lease_seq = $2
   FOR UPDATE";
const MARK_FAILED_RETRY_FENCED_SQL: &str = r"UPDATE usage_charge_queue
   SET status = 'failed', retry_count = retry_count + 1,
       error_message = $1, scheduled_at = $2
   WHERE id = $3 AND status = 'processing' AND lease_seq = $4";
const MARK_FAILED_DEAD_LETTER_FENCED_SQL: &str = r"UPDATE usage_charge_queue
   SET status = 'dead_letter', error_message = $1
   WHERE id = $2 AND status = 'processing' AND lease_seq = $3";
const MARK_USAGE_CHARGE_FAILED_SQL: &str = "UPDATE usage_records \
     SET charge_status = 'charge_failed' \
     WHERE charge_queue_id = $1 AND charge_status IS DISTINCT FROM 'charged'";

/// 一条待入队的扣费。
#[derive(Debug, Clone)]
pub struct ChargeEnqueue<'a> {
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub usage_record_id: i64,
    pub amount: Decimal,
    pub payload: Option<serde_json::Value>,
    /// 来源请求 span 的 W3C traceparent。
    pub trace_parent: Option<&'a str>,
}

#[derive(Clone, Debug)]
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
        if amount < Decimal::ZERO {
            return Err(BillingError::InvalidAmount(format!(
                "Charge enqueue amount must be non-negative, got {amount}"
            )));
        }
        let trace_parent = mazbri_core::trace_propagation::current_span_context()
            .map(|span_context| mazbri_core::trace_spans::traceparent_of(&span_context));
        let mut tx = self.db.begin().await?;
        let queue_id = self
            .enqueue_in_tx(
                &mut tx,
                ChargeEnqueue {
                    tenant_id,
                    account_id,
                    usage_record_id,
                    amount,
                    payload,
                    trace_parent: trace_parent.as_deref(),
                },
            )
            .await?;
        tx.commit().await?;
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
        item: ChargeEnqueue<'_>,
    ) -> Result<Uuid, BillingError> {
        let ChargeEnqueue {
            tenant_id,
            account_id,
            usage_record_id,
            amount,
            payload,
            trace_parent,
        } = item;
        if amount < Decimal::ZERO {
            return Err(BillingError::InvalidAmount(format!(
                "Charge enqueue amount must be non-negative, got {amount}"
            )));
        }
        let inserted: Option<Uuid> = sqlx::query_scalar(ENQUEUE_INSERT_SQL)
            .bind(tenant_id)
            .bind(account_id)
            .bind(usage_record_id)
            .bind(amount)
            .bind(payload)
            .bind(self.config.max_retries)
            .bind(trace_parent)
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
        }
        Ok(n)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn fetch_pending(&self, limit: i32) -> Result<Vec<ChargeQueueItem>, BillingError> {
        let mut rows = sqlx::query_as::<_, ChargeQueueItem>(CLAIM_PENDING_QUEUE_SQL)
            .bind(i64::from(limit))
            .bind(&self.config.worker_node_id)
            .fetch_all(&self.db)
            .await?;
        rows.sort_by_key(|r| r.created_at);
        if !rows.is_empty() {
            debug!("Fetched {} pending charge items for processing", rows.len());
        }
        Ok(rows)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn mark_completed(
        &self,
        queue_id: Uuid,
        lease_seq: i64,
        transaction_id: Uuid,
    ) -> Result<bool, BillingError> {
        let now = Utc::now();
        let mut tx = self.db.begin().await?;
        let updated = sqlx::query(MARK_COMPLETED_FENCED_SQL)
            .bind(queue_id)
            .bind(transaction_id)
            .bind(now)
            .bind(lease_seq)
            .execute(&mut *tx)
            .await?;
        if updated.rows_affected() == 0 {
            tx.commit().await?;
            debug!(
                %queue_id,
                lease_seq,
                "mark_completed fenced out: queue row no longer owned by this lease"
            );
            return Ok(false);
        }
        sqlx::query(
            "UPDATE usage_records SET charge_status = 'charged', charged_at = $1 \
             WHERE charge_queue_id = $2 AND (charge_status IS NULL OR charge_status != 'charged')",
        )
        .bind(now)
        .bind(queue_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        debug!("Marked charge queue item as completed: {}", queue_id);
        Ok(true)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn mark_failed(
        &self,
        queue_id: Uuid,
        lease_seq: i64,
        error_message: &str,
    ) -> Result<bool, BillingError> {
        let mut tx = self.db.begin().await?;
        let locked: Option<(Uuid, Uuid, Decimal, i32, i32)> =
            sqlx::query_as(MARK_FAILED_LOCK_FENCED_SQL)
                .bind(queue_id)
                .bind(lease_seq)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((tenant_id, account_id, amount, retry_count, max_retries)) = locked else {
            tx.commit().await?;
            debug!(
                %queue_id,
                lease_seq,
                "mark_failed fenced out: queue row no longer in processing for this lease"
            );
            return Ok(false);
        };
        if retry_count < max_retries {
            const MAX_RETRY_DELAY_SECS: i64 = 3600;
            let backoff_factor = 2i64.pow(u32::saturating_from(retry_count.clamp(0, 30)));
            let delay_secs =
                (self.config.retry_interval_secs * backoff_factor).min(MAX_RETRY_DELAY_SECS);
            let scheduled_at = Utc::now() + ChronoDuration::seconds(delay_secs);
            let res = sqlx::query(MARK_FAILED_RETRY_FENCED_SQL)
                .bind(error_message)
                .bind(scheduled_at)
                .bind(queue_id)
                .bind(lease_seq)
                .execute(&mut *tx)
                .await?;
            if res.rows_affected() == 0 {
                tx.commit().await?;
                return Ok(false);
            }
            sqlx::query(MARK_USAGE_CHARGE_FAILED_SQL)
                .bind(queue_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            warn!(
                "Charge queue item {} failed (attempt {}/{}, lease_seq={}), will retry at {}",
                queue_id,
                retry_count + 1,
                max_retries,
                lease_seq,
                scheduled_at
            );
            Ok(true)
        } else {
            let res = sqlx::query(MARK_FAILED_DEAD_LETTER_FENCED_SQL)
                .bind(error_message)
                .bind(queue_id)
                .bind(lease_seq)
                .execute(&mut *tx)
                .await?;
            if res.rows_affected() == 0 {
                tx.commit().await?;
                return Ok(false);
            }
            sqlx::query(MARK_USAGE_CHARGE_FAILED_SQL)
                .bind(queue_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                r"INSERT INTO charge_failures
                   (queue_id, tenant_id, account_id, amount, error_message, attempt_number)
                   VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(queue_id)
            .bind(tenant_id)
            .bind(account_id)
            .bind(amount)
            .bind(error_message)
            .bind(retry_count)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            warn!(
                "Charge queue item {} moved to dead letter after {} attempts (lease_seq={})",
                queue_id, max_retries, lease_seq
            );
            Ok(false)
        }
    }
}

#[cfg(test)]
mod enqueue_sql_tests {
    use super::{
        CLAIM_PENDING_QUEUE_SQL, ENQUEUE_INSERT_SQL, ENQUEUE_MARK_QUEUED_SQL,
        MARK_COMPLETED_FENCED_SQL, MARK_FAILED_DEAD_LETTER_FENCED_SQL, MARK_FAILED_LOCK_FENCED_SQL,
        MARK_FAILED_RETRY_FENCED_SQL, MARK_USAGE_CHARGE_FAILED_SQL,
    };

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

    #[test]
    fn claim_and_failure_transitions_enforce_lease_seq_fence() {
        assert!(CLAIM_PENDING_QUEUE_SQL.contains("lease_seq = q.lease_seq + 1"));
        assert!(CLAIM_PENDING_QUEUE_SQL.contains("RETURNING q.id"));
        assert!(CLAIM_PENDING_QUEUE_SQL.contains("q.lease_seq"));

        assert!(
            MARK_COMPLETED_FENCED_SQL
                .contains("WHERE id = $1 AND status = 'processing' AND lease_seq = $4")
        );
        assert!(
            MARK_FAILED_LOCK_FENCED_SQL
                .contains("WHERE id = $1 AND status = 'processing' AND lease_seq = $2")
        );
        assert!(
            MARK_FAILED_RETRY_FENCED_SQL
                .contains("WHERE id = $3 AND status = 'processing' AND lease_seq = $4")
        );
        assert!(
            MARK_FAILED_DEAD_LETTER_FENCED_SQL
                .contains("WHERE id = $2 AND status = 'processing' AND lease_seq = $3")
        );
        assert!(MARK_USAGE_CHARGE_FAILED_SQL.contains("IS DISTINCT FROM 'charged'"));
    }
}
