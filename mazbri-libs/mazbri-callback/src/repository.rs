use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct CallbackSubscription {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub product_id: Uuid,
    pub callback_url: String,
    pub secret_key: String,
    pub retry_policy: serde_json::Value,
    pub is_active: bool,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct CallbackDeliveryLog {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub event_id: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub retry_count: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub locked_at: Option<DateTime<Utc>>,
    pub lease_seq: i64,
    pub last_response_code: Option<i32>,
    pub last_error: Option<String>,
}

pub(crate) const FETCH_PENDING_TASKS_SQL: &str = r"
WITH picked AS (
    SELECT id
    FROM mazbri_platform.callback_delivery_logs
    WHERE (status IN ('pending', 'retrying') AND next_retry_at <= NOW())
       OR (status = 'processing' AND locked_at < NOW() - INTERVAL '5 minutes')
    ORDER BY COALESCE(next_retry_at, locked_at) ASC
    LIMIT $1
    FOR UPDATE SKIP LOCKED
)
UPDATE mazbri_platform.callback_delivery_logs d
SET status = 'processing',
    locked_at = NOW(),
    lease_seq = d.lease_seq + 1,
    updated_at = NOW()
FROM picked
WHERE d.id = picked.id
RETURNING d.id, d.subscription_id, d.event_id, d.payload, d.status, d.retry_count,
          d.next_retry_at, d.locked_at, d.lease_seq, d.last_response_code, d.last_error
";

pub(crate) const UPDATE_DELIVERY_STATUS_FENCED_SQL: &str = r"
UPDATE mazbri_platform.callback_delivery_logs
SET status = $2,
    retry_count = $3,
    next_retry_at = $4,
    locked_at = NULL,
    last_response_code = $5,
    last_error = $6,
    updated_at = NOW()
WHERE id = $1
  AND status = 'processing'
  AND lease_seq = $7
";

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn find_active_subscription(
    pool: &PgPool,
    tenant_id: Uuid,
    product_id: Uuid,
) -> sqlx::Result<Option<CallbackSubscription>> {
    sqlx::query_as::<_, CallbackSubscription>(
        r"
        SELECT id, tenant_id, product_id, callback_url, secret_key, retry_policy, is_active
        FROM mazbri_platform.callback_subscriptions
        WHERE tenant_id = $1 AND product_id = $2 AND is_active = true
        ",
    )
    .bind(tenant_id)
    .bind(product_id)
    .fetch_optional(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn enqueue_callback(
    pool: &PgPool,
    subscription_id: Uuid,
    event_id: &str,
    payload: &serde_json::Value,
) -> sqlx::Result<Uuid> {
    let row: (Uuid,) = sqlx::query_as(
        r"
        INSERT INTO mazbri_platform.callback_delivery_logs (subscription_id, event_id, payload, status, next_retry_at)
        VALUES ($1, $2, $3, 'pending', NOW())
        RETURNING id
        ",
    )
    .bind(subscription_id)
    .bind(event_id)
    .bind(payload)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Claims due `'pending'` / `'retrying'` tasks as well as stale `'processing'` tasks
/// whose lease (`locked_at`) expired over 5 minutes ago, incrementing `lease_seq`.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn fetch_pending_tasks(
    pool: &PgPool,
    limit: i64,
) -> sqlx::Result<Vec<CallbackDeliveryLog>> {
    sqlx::query_as::<_, CallbackDeliveryLog>(FETCH_PENDING_TASKS_SQL)
        .bind(limit)
        .fetch_all(pool)
        .await
}

#[derive(Debug, Clone, Copy)]
pub struct DeliveryStatusUpdate<'a> {
    pub id: Uuid,
    pub lease_seq: i64,
    pub status: &'a str,
    pub retry_count: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub response_code: Option<i32>,
    pub error: Option<&'a str>,
}

/// Updates callback delivery status fenced by `(status = 'processing', lease_seq)`.
/// Returns `Ok(true)` if the row was updated, or `Ok(false)` if another worker already reclaimed the lease.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_delivery_status(
    pool: &PgPool,
    update: DeliveryStatusUpdate<'_>,
) -> sqlx::Result<bool> {
    let res = sqlx::query(UPDATE_DELIVERY_STATUS_FENCED_SQL)
        .bind(update.id)
        .bind(update.status)
        .bind(update.retry_count)
        .bind(update.next_retry_at)
        .bind(update.response_code)
        .bind(update.error)
        .bind(update.lease_seq)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::{FETCH_PENDING_TASKS_SQL, UPDATE_DELIVERY_STATUS_FENCED_SQL};

    #[test]
    fn callback_queries_reclaim_stale_processing_and_fence_on_lease_seq() {
        assert!(FETCH_PENDING_TASKS_SQL.contains("status = 'processing'"));
        assert!(FETCH_PENDING_TASKS_SQL.contains("locked_at < NOW() - INTERVAL '5 minutes'"));
        assert!(FETCH_PENDING_TASKS_SQL.contains("lease_seq = d.lease_seq + 1"));
        assert!(UPDATE_DELIVERY_STATUS_FENCED_SQL.contains("AND status = 'processing'"));
        assert!(UPDATE_DELIVERY_STATUS_FENCED_SQL.contains("AND lease_seq = $7"));
    }
}
