//! 计费链路运维只读查询：所有计数都以 `created_at` 时间窗过滤（走 `idx_*_created_at`），
//! 不做全表扫描。

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StatusCount {
    pub status: String,
    pub count: i64,
}

/// 明细分页（limit 已收敛到 1–200）。
#[derive(Debug, Clone, Copy)]
pub struct Page {
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ChargeFailureRow {
    pub id: i64,
    pub queue_id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub amount: Decimal,
    pub error_message: String,
    pub attempt_number: i32,
    pub queue_status: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DeadLetterRow {
    pub source: String,
    pub id: String,
    pub tenant_id: uuid::Uuid,
    pub retry_count: i32,
    pub error_message: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn outbox_by_status(
    pool: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<StatusCount>, sqlx::Error> {
    sqlx::query_as::<_, StatusCount>(
        "SELECT status::text AS status, count(*) AS count FROM usage_outbox \
         WHERE created_at >= $1 GROUP BY status ORDER BY status",
    )
    .bind(since)
    .fetch_all(pool)
    .await
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn queue_by_status(
    pool: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<StatusCount>, sqlx::Error> {
    sqlx::query_as::<_, StatusCount>(
        "SELECT status::text AS status, count(*) AS count FROM usage_charge_queue \
         WHERE created_at >= $1 GROUP BY status ORDER BY status",
    )
    .bind(since)
    .fetch_all(pool)
    .await
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn ledger_charge_count(pool: &PgPool, since: DateTime<Utc>) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM ledger_entries WHERE created_at >= $1 AND entry_type = 'CHARGE'",
    )
    .bind(since)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn dead_letter_count(pool: &PgPool, since: DateTime<Utc>) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM usage_outbox \
                 WHERE created_at >= $1 AND status = 'dead_letter') \
              + (SELECT count(*) FROM usage_charge_queue \
                 WHERE created_at >= $1 AND status = 'dead_letter')",
    )
    .bind(since)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn charge_failure_count(pool: &PgPool, since: DateTime<Utc>) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM charge_failures WHERE created_at >= $1")
        .bind(since)
        .fetch_one(pool)
        .await
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn recent_charge_failures(
    pool: &PgPool,
    since: DateTime<Utc>,
    page: Page,
) -> Result<Vec<ChargeFailureRow>, sqlx::Error> {
    sqlx::query_as::<_, ChargeFailureRow>(
        "SELECT f.id, f.queue_id, f.tenant_id, f.amount, f.error_message, f.attempt_number, \
         q.status::text AS queue_status, f.created_at \
         FROM charge_failures f LEFT JOIN usage_charge_queue q ON q.id = f.queue_id \
         WHERE f.created_at >= $1 ORDER BY f.created_at DESC, f.id DESC LIMIT $2 OFFSET $3",
    )
    .bind(since)
    .bind(page.limit)
    .bind(page.offset)
    .fetch_all(pool)
    .await
}

/// 死信：outbox 与扣费队列中 `dead_letter` 状态的条目。
///
/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn recent_dead_letters(
    pool: &PgPool,
    since: DateTime<Utc>,
    page: Page,
) -> Result<Vec<DeadLetterRow>, sqlx::Error> {
    sqlx::query_as::<_, DeadLetterRow>(
        "SELECT * FROM ( \
           SELECT 'usage_outbox' AS source, id::text AS id, tenant_id, retry_count, \
                  error_message, created_at \
           FROM usage_outbox WHERE created_at >= $1 AND status = 'dead_letter' \
           UNION ALL \
           SELECT 'usage_charge_queue' AS source, id::text AS id, tenant_id, retry_count, \
                  error_message, created_at \
           FROM usage_charge_queue WHERE created_at >= $1 AND status = 'dead_letter' \
         ) d ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3",
    )
    .bind(since)
    .bind(page.limit)
    .bind(page.offset)
    .fetch_all(pool)
    .await
}
