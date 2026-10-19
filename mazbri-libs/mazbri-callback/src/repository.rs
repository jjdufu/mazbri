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
    pub last_response_code: Option<i32>,
    pub last_error: Option<String>,
}
pub async fn find_active_subscription(
    pool: &PgPool,
    tenant_id: Uuid,
    product_id: Uuid,
) -> sqlx::Result<Option<CallbackSubscription>> {
    sqlx::query_as::<_, CallbackSubscription>(
        r#"
        SELECT id, tenant_id, product_id, callback_url, secret_key, retry_policy, is_active
        FROM mazbri_platform.callback_subscriptions
        WHERE tenant_id = $1 AND product_id = $2 AND is_active = true
        "#,
    )
    .bind(tenant_id)
    .bind(product_id)
    .fetch_optional(pool)
    .await
}
pub async fn enqueue_callback(
    pool: &PgPool,
    subscription_id: Uuid,
    event_id: &str,
    payload: &serde_json::Value,
) -> sqlx::Result<Uuid> {
    let row: (Uuid,) = sqlx::query_as(
            r#"
        INSERT INTO mazbri_platform.callback_delivery_logs (subscription_id, event_id, payload, status, next_retry_at)
        VALUES ($1, $2, $3, 'pending', NOW())
        RETURNING id
        "#,
        )
        .bind(subscription_id)
        .bind(event_id)
        .bind(payload)
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}
pub async fn fetch_pending_tasks(
    pool: &PgPool,
    limit: i64,
) -> sqlx::Result<Vec<CallbackDeliveryLog>> {
    sqlx::query_as::<
        _,
        CallbackDeliveryLog,
    >(
            r#"
        WITH picked AS (
            SELECT id
            FROM mazbri_platform.callback_delivery_logs
            WHERE status IN ('pending', 'retrying') AND next_retry_at <= NOW()
            ORDER BY next_retry_at ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE mazbri_platform.callback_delivery_logs d
        SET status = 'processing', updated_at = NOW()
        FROM picked
        WHERE d.id = picked.id
        RETURNING d.id, d.subscription_id, d.event_id, d.payload, d.status, d.retry_count, d.next_retry_at, d.last_response_code, d.last_error
        "#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await
}
pub async fn update_delivery_status(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    retry_count: i32,
    next_retry_at: Option<DateTime<Utc>>,
    response_code: Option<i32>,
    error: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
            r#"
        UPDATE mazbri_platform.callback_delivery_logs
        SET status = $2, retry_count = $3, next_retry_at = $4, last_response_code = $5, last_error = $6, updated_at = NOW()
        WHERE id = $1
        "#,
        )
        .bind(id)
        .bind(status)
        .bind(retry_count)
        .bind(next_retry_at)
        .bind(response_code)
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}
